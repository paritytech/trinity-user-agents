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
    /// calling task is cancelled. Returns without calling it when the
    /// balance cannot be followed, and returns early if the balance stream
    /// ends or fails; either way the check goes by the last value it got,
    /// and is skipped without one.
    func followSpendable(_ onChange: @escaping @MainActor (Decimal) -> Void) async
    var branding: FundingProviderBranding { get }

    /// Presents over whatever is on screen; false when nothing is.
    func present(_ controller: UIViewController) -> Bool
    /// The provider product's page at `route`, as the app shows products.
    func providerPage(providerId: ProductId, route: String, title: String) -> UIViewController?
    /// A session changed, for the CASH card's lists.
    func fundingActivityChanged()
    /// A passing notice over whatever is on screen.
    func showNotice(_ message: String)
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
        productId _: ProductId?,
        intent: String,
        direction: FundingDirection,
        amount: U128?
    ) async -> FundingPresentOutcome {
        await present(intent: intent, direction: direction, amount: amount)
    }

    func presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget
    ) async -> FundingFrameOutcome {
        await frame(providerId: providerId, intent: intent, target: target)
    }

    func closeProviderFrame(providerId: ProductId, intent: String) {
        Task { @MainActor in coordinator?.closeFrame(providerId: providerId, intent: intent) }
    }

    func fundingSessionChanged(intent: String) {
        Task { @MainActor in coordinator?.sessionChanged(intent: intent) }
    }

    func fundingOptionsChanged(intent: String) {
        Task { @MainActor in coordinator?.optionsChanged(intent: intent) }
    }

    func fundingQuotesChanged(intent: String) {
        Task { @MainActor in coordinator?.quotesChanged(intent: intent) }
    }

    func workerCrashing(productId: ProductId) {
        Task { @MainActor in await coordinator?.workerCrashing(productId: productId) }
    }

    func reopen(intent: String) {
        Task { @MainActor in coordinator?.reopen(intent: intent) }
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
    func frame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget
    ) async -> FundingFrameOutcome {
        guard let coordinator else { return .dismissed }
        switch target {
        case let .app(route):
            return await coordinator.presentFrame(
                providerId: providerId,
                intent: intent,
                route: route,
                awaitsUserTransfer: runtime?.fundingProgress(intent: intent)?.awaitsUserTransfer ?? false
            )
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
    private let environment: FundingOverlayEnvironment
    private var flows: [String: FundingFlowModel] = [:]
    private var frames: [String: FundingProviderFrameController] = [:]
    private var pages: [String: FundingWebPageController] = [:]

    init(runtime: FundingRuntime, environment: FundingOverlayEnvironment) {
        self.runtime = runtime
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

    /// Shows a session again: one still open on its progress screen, with the
    /// provider's screen over it while the provider still waits on one, or an
    /// ended one on its detail screen.
    func reopen(intent: String) {
        guard flows[intent] == nil else { return }
        guard runtime?.fundingSessionOpen(intent: intent) == true else {
            showEnded(intent: intent)
            return
        }
        guard let session = runtime?.fundingSession(intent: intent),
              let model = makeModel(intent: intent, direction: session.direction, amount: session.amount)
        else { return }

        model.resume(session)
        let presented = presentSheet(for: model) { [weak self] in
            guard let frame = self?.frames[intent], frame.presentingViewController == nil else { return }
            _ = self?.environment.present(frame)
        }
        if presented {
            acknowledgeShown(intent: intent)
        }
    }

    /// An ended session, drawn from its summary in the core's history, with
    /// the steps the core reported while it still holds the session.
    func showEnded(intent: String) {
        guard let runtime,
              let summary = runtime.fundingSummary(intent: intent),
              let ended = FundingEndedSession(
                  summary: summary,
                  session: runtime.fundingSession(intent: intent),
                  progress: runtime.fundingProgress(intent: intent)
              )
        else { return }

        let sheet = FundingEndedSheetController(
            ended: ended,
            cash: environment.cash,
            branding: environment.branding
        )
        sheet.onStartOver = { [weak self, weak sheet] in
            sheet?.dismissIfPresented { self?.startOver(direction: summary.direction) }
        }
        if environment.present(sheet) {
            acknowledgeShown(intent: intent)
        }
    }

    /// The provider closed its frame for session `intent`: the frame answers
    /// `.closed`, and the session's progress shows in its place when the frame
    /// was on screen. A hidden frame only answers.
    func closeFrame(providerId: ProductId, intent: String) {
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
    /// waiting first. A session waiting on the user's own transfer offers to
    /// say the funds were sent.
    func presentFrame(
        providerId: ProductId,
        intent: String,
        route: String,
        awaitsUserTransfer: Bool
    ) async -> FundingFrameOutcome {
        let brand = await environment.branding.brand(for: providerId)
        guard !Task.isCancelled,
              let page = environment.providerPage(providerId: providerId, route: route, title: brand.name)
        else { return .dismissed }

        let presented = Presented<FundingProviderFrameController>()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                let frame = FundingProviderFrameController(
                    providerId: providerId,
                    page: page,
                    showsSentFunds: awaitsUserTransfer
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
    func presentPage(providerId: ProductId, intent: String, url: String, returnUrl: String?) async
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
        if flows[intent] != nil {
            acknowledgeShown(intent: intent)
        }
        environment.fundingActivityChanged()
    }

    /// Says so as an error notice over whatever is on screen, a session
    /// sheet the product serves included. The sheet's own toast is for
    /// confirmations.
    func workerCrashing(productId: ProductId) async {
        let name = await environment.branding.brand(for: productId).name
        environment.showNotice(String(localized: .Funding.providerCrashing(provider: name)))
    }

    func optionsChanged(intent: String) {
        flows[intent]?.refreshOptions()
    }

    func quotesChanged(intent: String) {
        flows[intent]?.refreshQuotes()
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
            service: FundingFlowService(runtime: runtime, branding: environment.branding),
            logger: Logger.shared
        )
        if direction == .out {
            model.balanceTask = Task { [environment, weak model] in
                await environment.followSpendable { model?.spendable = $0 }
            }
        }
        return model
    }

    /// Whether the sheet went on screen; when nothing could present it the
    /// session's model is closed.
    @discardableResult
    func presentSheet(for model: FundingFlowModel, onAppear: (() -> Void)? = nil) -> Bool {
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

        guard environment.present(sheet) else {
            model.close()
            return false
        }
        return true
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
        Task { await runtime.acknowledgeShown(intent: intent, logger: Logger.shared) }
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
