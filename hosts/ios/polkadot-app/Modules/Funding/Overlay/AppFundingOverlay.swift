import Foundation
import SwiftUI
import TrUAPIHost
import UIKit

/// What the overlay needs from the app around it.
@MainActor
protocol FundingOverlayEnvironment: AnyObject {
    var cash: FundingCash { get }
    /// Calls `onChange` with what a payment can spend, for the withdraw
    /// screen's balance check, now and on every balance change, until the
    /// calling task is cancelled.
    func followSpendable(_ onChange: @escaping @MainActor (Decimal) -> Void) async
    var branding: FundingProviderBranding { get }

    /// Presents over whatever is on screen; false when nothing is.
    func present(_ controller: UIViewController) -> Bool
    /// The provider product's page at `route`, as the app shows products.
    func providerPage(providerId: String, route: String, title: String) -> UIViewController?
    /// A session changed, for the CASH card's lists.
    func fundingActivityChanged()
}

/// The runtime's funding overlay: the sheet a session is started from, and
/// the frame a provider shows its own screens in.
final class AppFundingOverlay: FundingOverlayPresenting, @unchecked Sendable {
    private weak var runtime: FundingRuntime?
    private let makeEnvironment: @MainActor () -> FundingOverlayEnvironment
    @MainActor private var coordinatorStorage: FundingOverlayCoordinator?

    /// Built with the runtime, off the main thread, so what the overlay needs
    /// on screen is assembled the first time it is shown.
    init(runtime: FundingRuntime, environment: @escaping @MainActor () -> FundingOverlayEnvironment) {
        self.runtime = runtime
        makeEnvironment = environment
    }

    func presentFunding(
        productId _: String?,
        intent: String,
        direction: FundingDirection,
        amount: U128?
    ) async -> FundingPresentOutcome {
        await present(intent: intent, direction: direction, amount: amount)
    }

    func presentProviderFrame(
        providerId: String,
        intent: String,
        target: FundingFrameTarget
    ) async -> FundingFrameOutcome {
        await frame(providerId: providerId, intent: intent, target: target)
    }

    func closeProviderFrame(providerId: String, intent: String) {
        Task { @MainActor in coordinator?.closeFrame(providerId: providerId, intent: intent) }
    }

    func fundingSessionChanged(intent: String, status _: HostFundingStatusSubscribeItem) {
        Task { @MainActor in coordinator?.sessionChanged(intent: intent) }
    }

    func fundingOptionsChanged(intent: String) {
        Task { @MainActor in coordinator?.optionsChanged(intent: intent) }
    }

    func fundingQuotesChanged(intent: String, quotes: FundingQuoteList) {
        Task { @MainActor in coordinator?.quotesChanged(intent: intent, quotes: quotes) }
    }

    func reopen(intent: String) {
        Task { @MainActor in coordinator?.reopen(intent: intent) }
    }

    func showEnded(_ record: FundingRecord) {
        Task { @MainActor in coordinator?.showEnded(record) }
    }

    func showPending(direction: FundingDirection) {
        Task { @MainActor in coordinator?.showPending(direction: direction) }
    }

    func showHistory() {
        Task { @MainActor in coordinator?.showHistory() }
    }
}

private extension AppFundingOverlay {
    @MainActor
    var coordinator: FundingOverlayCoordinator? {
        if let coordinatorStorage { return coordinatorStorage }
        guard let runtime else { return nil }

        let created = FundingOverlayCoordinator(runtime: runtime, environment: makeEnvironment())
        coordinatorStorage = created
        return created
    }

    @MainActor
    func present(intent: String, direction: FundingDirection, amount: U128?) async -> FundingPresentOutcome {
        guard let coordinator else { return .dismissed }
        return await coordinator.presentFunding(intent: intent, direction: direction, amount: amount)
    }

    @MainActor
    func frame(providerId: String, intent: String, target: FundingFrameTarget) async -> FundingFrameOutcome {
        guard let coordinator else { return .dismissed }
        switch target {
        case let .app(route):
            return await coordinator.presentFrame(providerId: providerId, intent: intent, route: route)
        case let .page(url, returnUrl):
            return await coordinator.presentPage(providerId: providerId, intent: intent, url: url, returnUrl: returnUrl)
        }
    }
}

/// Holds the overlays on screen, keyed by session, so the core's updates
/// reach the one they belong to.
@MainActor
final class FundingOverlayCoordinator {
    private weak var runtime: FundingRuntime?
    private let book: FundingSessionBook
    private let environment: FundingOverlayEnvironment
    private var flows: [String: FundingFlowModel] = [:]
    private var frames: [String: FundingProviderFrameController] = [:]
    private var pages: [String: FundingWebPageController] = [:]

    init(runtime: FundingRuntime, environment: FundingOverlayEnvironment, book: FundingSessionBook = .shared) {
        self.runtime = runtime
        self.book = book
        self.environment = environment
    }

    /// Answers `.dismissed` and takes the sheet down if the core stops
    /// waiting first.
    func presentFunding(intent: String, direction: FundingDirection, amount: U128?) async -> FundingPresentOutcome {
        guard !Task.isCancelled,
              let model = makeModel(intent: intent, direction: direction, amount: amount)
        else { return .dismissed }

        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                var answered = false
                model.onOutcome = { outcome in
                    guard !answered else { return }
                    answered = true
                    continuation.resume(returning: outcome)
                }

                presentSheet(for: model)
            }
        } onCancel: {
            Task { @MainActor [weak model] in model?.close() }
        }
    }

    /// Shows a session the core holds: one the user left running on its
    /// progress screen, with the provider's screen over it while the provider
    /// still waits on one, or an ended one on its ended screen.
    func reopen(intent: String) {
        guard flows[intent] == nil,
              let session = book.session(intent: intent),
              let model = makeModel(intent: intent, direction: session.direction, amount: session.amount)
        else { return }

        model.resume(session)
        presentSheet(for: model) { [weak self] in
            guard let frame = self?.frames[intent], frame.presentingViewController == nil else { return }
            _ = self?.environment.present(frame)
        }
        acknowledgeShown(intent: intent)
    }

    /// An ended session the core did not push this run is drawn from its
    /// history record.
    func showEnded(_ record: FundingRecord) {
        if book.session(intent: record.intent) != nil {
            reopen(intent: record.intent)
            return
        }

        let sheet = FundingEndedSheetController(
            ended: FundingEndedSession(record: record),
            cash: environment.cash,
            branding: environment.branding
        )
        sheet.onStartOver = { [weak self, weak sheet] in
            sheet?.dismissIfPresented { self?.startOver(direction: record.direction) }
        }
        _ = environment.present(sheet)
    }

    /// The provider closed its frame for session `intent`: the frame answers
    /// `.closed`, and the session's progress shows in its place when the frame
    /// was on screen. A hidden frame only answers.
    func closeFrame(providerId: String, intent: String) {
        if let frame = frames[intent], frame.providerId == providerId {
            let shown = frame.presentingViewController != nil
            frame.finish(.closed, dismissed: { [weak self] in
                if shown { self?.showProgress(intent: intent) }
            })
        } else if let page = pages[intent], page.providerId == providerId {
            let shown = page.presentingViewController != nil
            page.finish(.closed, dismissed: { [weak self] in
                if shown { self?.showProgress(intent: intent) }
            })
        }
    }

    /// The sessions in flight in `direction`, over whatever is on screen. A
    /// new session starts once the sheet is gone.
    func showPending(direction: FundingDirection) {
        let sheet = FundingListSheetController()
        sheet.show(FundingPendingView(
            direction: direction,
            cash: environment.cash,
            onHistory: { [weak self] in self?.showHistory() },
            onNew: { [weak self, weak sheet] in
                sheet?.dismissIfPresented { self?.startOver(direction: direction) }
            }
        ))
        _ = environment.present(sheet)
    }

    /// Every session, in flight and ended.
    func showHistory() {
        let sheet = FundingListSheetController()
        sheet.show(FundingHistoryView(
            cash: environment.cash,
            onBack: { [weak sheet] in sheet?.dismissIfPresented {} }
        ))
        _ = environment.present(sheet)
    }

    /// Answers `.dismissed` and takes the frame down if the core stops
    /// waiting first.
    func presentFrame(providerId: String, intent: String, route: String) async -> FundingFrameOutcome {
        let brand = await environment.branding.brand(for: providerId)
        guard !Task.isCancelled,
              let page = environment.providerPage(providerId: providerId, route: route, title: brand.name)
        else { return .dismissed }

        let rail = flows[intent]?.rail ?? book.session(intent: intent)?.choice?.rail ?? .card

        let presented = Presented<FundingProviderFrameController>()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                let frame = FundingProviderFrameController(
                    providerId: providerId,
                    page: page,
                    showsSentFunds: rail == .bank
                ) { [weak self] outcome in
                    if let self, frames[intent] === presented.controller {
                        frames[intent] = nil
                    }
                    continuation.resume(returning: outcome)
                }
                presented.controller = frame
                frames[intent] = frame

                if !environment.present(frame) {
                    frame.finish(.dismissed)
                }
            }
        } onCancel: {
            Task { @MainActor in presented.controller?.finish(.dismissed) }
        }
    }

    /// Shows an external page in a frame the host owns, answering
    /// `.returned` when it reaches `returnUrl` and `.dismissed` when the user
    /// closes it.
    func presentPage(providerId: String, intent: String, url: String, returnUrl: String?) async
        -> FundingFrameOutcome {
        guard !Task.isCancelled, let pageURL = URL(string: url) else { return .dismissed }

        let presented = Presented<FundingWebPageController>()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                let page = FundingWebPageController(
                    providerId: providerId,
                    url: pageURL,
                    returnURL: returnUrl.flatMap(URL.init(string:))
                ) { [weak self] outcome in
                    if let self, pages[intent] === presented.controller {
                        pages[intent] = nil
                    }
                    continuation.resume(returning: outcome)
                }
                presented.controller = page
                pages[intent] = page
                if !environment.present(page) {
                    page.finish(.dismissed)
                }
            }
        } onCancel: {
            Task { @MainActor in presented.controller?.finish(.dismissed) }
        }
    }

    func sessionChanged(intent: String) {
        flows[intent]?.refreshSession()
        if let frame = frames[intent], book.paymentMoved(intent: intent) {
            frame.finish(.closed)
        }
        if flows[intent] != nil {
            acknowledgeShown(intent: intent)
        }
        environment.fundingActivityChanged()
    }

    func optionsChanged(intent: String) {
        flows[intent]?.refreshOptions()
    }

    func quotesChanged(intent: String, quotes: FundingQuoteList) {
        flows[intent]?.receive(quotes: quotes)
    }
}

private extension FundingOverlayCoordinator {
    func makeModel(intent: String, direction: FundingDirection, amount: U128?) -> FundingFlowModel? {
        guard let runtime else { return nil }

        let model = FundingFlowModel(
            context: .init(
                intent: intent,
                direction: direction,
                amount: amount,
                cash: environment.cash
            ),
            runtime: runtime,
            book: book,
            branding: environment.branding
        )
        if direction == .out {
            model.balanceTask = Task { [environment, weak model] in
                await environment.followSpendable { model?.spendable = $0 }
            }
        }
        return model
    }

    func presentSheet(for model: FundingFlowModel, onAppear: (() -> Void)? = nil) {
        let intent = model.intent
        let sheet = FundingSheetController(model: model, onAppear: onAppear)
        model.onClose = { [weak self, weak sheet] in
            sheet?.dismissIfPresented()
            self?.flows[intent] = nil
        }
        model.onStartOver = { [weak self, weak sheet, direction = model.direction] in
            self?.flows[intent] = nil
            sheet?.dismissIfPresented { self?.startOver(direction: direction) }
        }
        model.onHistory = { [weak self, weak sheet] in
            self?.flows[intent] = nil
            sheet?.dismissIfPresented { self?.showHistory() }
        }
        flows[intent] = model

        if !environment.present(sheet) {
            model.close()
        }
    }

    /// The session sheet's progress screen for `intent`: the open sheet, or
    /// the session shown again when none is.
    func showProgress(intent: String) {
        if let model = flows[intent] {
            model.showProgress()
        } else {
            reopen(intent: intent)
        }
    }

    /// An ended session on screen has told the user how it went.
    func acknowledgeShown(intent: String) {
        guard let runtime else { return }
        Task { [book] in await book.acknowledgeShown(intent: intent, runtime: runtime) }
    }

    /// A new session the same way, opened once the sheet before it is gone.
    func startOver(direction: FundingDirection) {
        guard let runtime else { return }

        Task {
            do {
                _ = try await runtime.openFunding(direction: direction, amount: nil)
            } catch {
                Logger.shared.error("[funding] starting over failed: \(error)")
            }
        }
    }
}

/// The frame or page one presenting call showed, held weakly so cancelling
/// that call takes down only its own screen.
@MainActor
private final class Presented<Controller: AnyObject> {
    weak var controller: Controller?
}
