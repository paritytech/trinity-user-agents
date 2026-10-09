import Foundation
import SwiftUI
import TrUAPIHost
import UIKit

/// What the overlay needs from the app around it.
@MainActor
protocol FundingOverlayEnvironment: AnyObject {
    var cash: FundingCash { get }
    /// What the user can spend now, for the withdraw screen's balance check.
    var spendable: Decimal? { get }
    var branding: FundingProviderBranding { get }

    /// Presents over whatever is on screen; false when nothing is.
    func present(_ controller: UIViewController) -> Bool
    /// The provider product's page at `route`, as the app shows products.
    func providerPage(providerId: String, route: String, title: String) -> UIViewController?
    /// A session the core holds changed, for the CASH card's lists.
    func fundingSessionChanged(intent: String)
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

    func presentProviderFrame(providerId: String, intent: String, route: String) async -> FundingFrameOutcome {
        await frame(providerId: providerId, intent: intent, route: route)
    }

    func fundingSessionChanged(intent: String, status _: HostFundingStatusSubscribeItem) {
        Task { @MainActor in coordinator?.sessionChanged(intent: intent) }
    }

    func fundingQuoteChanged(intent: String, row: FundingQuoteRow) {
        Task { @MainActor in coordinator?.quoteChanged(intent: intent, row: row) }
    }

    func reopen(intent: String) {
        Task { @MainActor in coordinator?.reopen(intent: intent) }
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
    func frame(providerId: String, intent: String, route: String) async -> FundingFrameOutcome {
        guard let coordinator else { return .dismissed }
        return await coordinator.presentFrame(providerId: providerId, intent: intent, route: route)
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

    init(runtime: FundingRuntime, environment: FundingOverlayEnvironment) {
        self.runtime = runtime
        self.environment = environment
    }

    func presentFunding(intent: String, direction: FundingDirection, amount: U128?) async -> FundingPresentOutcome {
        guard let model = makeModel(intent: intent, direction: direction, amount: amount) else { return .dismissed }

        return await withCheckedContinuation { continuation in
            var answered = false
            model.onOutcome = { outcome in
                guard !answered else { return }
                answered = true
                continuation.resume(returning: outcome)
            }

            presentSheet(for: model)
        }
    }

    /// Shows a session the user left running: its deposit screen, with the
    /// provider's screen over it while the provider still waits on one.
    func reopen(intent: String) {
        guard flows[intent] == nil,
              let session = runtime?.fundingSession(intent: intent), session.stage.isOpen,
              let model = makeModel(intent: intent, direction: session.direction, amount: session.amount)
        else { return }

        model.resume(session)
        presentSheet(for: model) { [weak self] in
            guard let frame = self?.frames[intent], frame.presentingViewController == nil else { return }
            _ = self?.environment.present(frame)
        }
    }

    func presentFrame(providerId: String, intent: String, route: String) async -> FundingFrameOutcome {
        let brand = await environment.branding.brand(for: providerId)
        guard let page = environment.providerPage(providerId: providerId, route: route, title: brand.name) else {
            return .dismissed
        }

        let rail = flows[intent]?.rail ?? runtime?.fundingSession(intent: intent)?.choice?.rail ?? .card

        return await withCheckedContinuation { continuation in
            let frame = FundingProviderFrameController(
                page: page,
                showsSentFunds: rail == .bank
            ) { [weak self] outcome in
                self?.frames[intent] = nil
                continuation.resume(returning: outcome)
            }
            frames[intent] = frame

            if !environment.present(frame) {
                frame.finish(.dismissed)
            }
        }
    }

    func sessionChanged(intent: String) {
        flows[intent]?.refreshSession()
        if let frame = frames[intent], let runtime, Self.paymentMoved(runtime: runtime, intent: intent) {
            frame.finish(.closed)
        }
        environment.fundingSessionChanged(intent: intent)
    }

    func quoteChanged(intent: String, row: FundingQuoteRow) {
        flows[intent]?.receive(row: row)
    }
}

private extension FundingOverlayCoordinator {
    func makeModel(intent: String, direction: FundingDirection, amount: U128?) -> FundingFlowModel? {
        guard let runtime else { return nil }

        return FundingFlowModel(
            context: .init(
                intent: intent,
                direction: direction,
                amount: amount,
                cash: environment.cash,
                spendable: environment.spendable
            ),
            runtime: runtime,
            branding: environment.branding
        )
    }

    func presentSheet(for model: FundingFlowModel, onAppear: (() -> Void)? = nil) {
        let intent = model.intent
        let sheet = FundingSheetController(model: model, onAppear: onAppear)
        model.onClose = { [weak self, weak sheet] in
            sheet?.dismissIfPresented()
            self?.flows[intent] = nil
        }
        flows[intent] = model

        if !environment.present(sheet) {
            model.close()
        }
    }

    /// The provider's screen has done its part once the payment is seen or
    /// the session is over, which is when the frame closes itself.
    static func paymentMoved(runtime: FundingRuntime, intent: String) -> Bool {
        guard let session = runtime.fundingSession(intent: intent) else { return true }
        if case .open = session.stage {
            let reached = runtime.fundingProgress(intent: intent)?.steps
                .filter { $0.reachedAtMs != nil }
                .map(\.step) ?? []
            return reached.contains { $0 != .started }
        }
        return true
    }
}

/// The overlay sheet. A swipe down is the user leaving, the same as Close.
final class FundingSheetController: UIHostingController<FundingSheetView>, UIAdaptivePresentationControllerDelegate {
    private let model: FundingFlowModel
    private var onAppear: (() -> Void)?

    init(model: FundingFlowModel, onAppear: (() -> Void)? = nil) {
        self.model = model
        self.onAppear = onAppear
        super.init(rootView: FundingSheetView(model: model))

        modalPresentationStyle = .pageSheet
        sheetPresentationController?.detents = [.custom { $0.maximumDetentValue * 0.92 }]
        sheetPresentationController?.prefersGrabberVisible = true
        sheetPresentationController?.preferredCornerRadius = 32
        presentationController?.delegate = self
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .clear
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        let onAppear = onAppear
        self.onAppear = nil
        onAppear?()
    }

    /// Dismisses from the presenter, so a provider's screen still over the
    /// sheet goes with it.
    func dismissIfPresented() {
        guard let presenter = presentingViewController, !isBeingDismissed else { return }
        presenter.dismiss(animated: true)
    }

    func presentationControllerDidDismiss(_: UIPresentationController) {
        model.close()
    }
}
