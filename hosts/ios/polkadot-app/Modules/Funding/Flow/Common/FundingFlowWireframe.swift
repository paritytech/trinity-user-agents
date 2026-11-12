import Foundation
import TrUAPIHost
import UIKit
import UIKit_iOS

/// What a funding flow needs from the app around it.
@MainActor
protocol FundingFlowEnvironment: AnyObject {
    var cash: FundingCash { get }
    var branding: FundingProviderBranding { get }
    /// Calls `onChange` with what a payment can spend, now and on every
    /// balance change, until the calling task is cancelled. Returns without
    /// calling it when the balance cannot be followed.
    func followSpendable(_ onChange: @escaping @MainActor (Decimal) -> Void) async
}

/// Moves between one funding flow's screens, on the sheet the flow is shown
/// in. Every screen's wireframe protocol is answered here, since they all
/// move on the same sheet.
@MainActor
final class FundingFlowWireframe {
    let context: FundingFlowContext
    let service: FundingServiceProtocol
    let environment: FundingFlowEnvironment
    private(set) weak var navigation: FundingFlowNavigationController?

    /// The flow's sheet went away.
    var onClose: (() -> Void)?
    /// Leaves the flow for the history of every session.
    var onHistory: (() -> Void)?
    /// Leaves the flow for a new session the same way.
    var onStartOver: ((FundingDirection) -> Void)?

    init(context: FundingFlowContext, service: FundingServiceProtocol, environment: FundingFlowEnvironment) {
        self.context = context
        self.service = service
        self.environment = environment
    }

    /// The sheet with `screens` on it, the last one showing.
    func makeSheet(_ screens: [FundingFlowScreen]) -> FundingFlowNavigationController {
        let sheet = FundingFlowNavigationController(screens: screens.map(makeScreen))
        sheet.onUserDismiss = { [weak self] in self?.didLeave() }
        navigation = sheet
        return sheet
    }

    /// The screens on the sheet now, bottom first.
    var screens: [FundingFlowScreen] {
        navigation?.viewControllers.compactMap { ($0 as? FundingFlowScreenTagged)?.screen } ?? []
    }
}

/// The screens of a funding flow.
enum FundingFlowScreen: Hashable {
    case amount
    case summary
    case fees
    case country
    case providers
    case network
    case token
    case deposit
    case progress
    case cancelConfirm
}

/// A screen's controller says which flow screen it is.
@MainActor
protocol FundingFlowScreenTagged: AnyObject {
    var screen: FundingFlowScreen { get }
}

// MARK: - Moving between screens

extension FundingFlowWireframe {
    func show(_ screen: FundingFlowScreen) {
        navigation?.pushViewController(makeScreen(screen), animated: true)
    }

    func back() {
        guard let navigation, navigation.viewControllers.count > 1 else {
            close()
            return
        }
        navigation.popViewController(animated: true)
    }

    /// The session's progress screen: the one on the sheet, or a new one in
    /// place of every screen when there is none.
    func showProgress() {
        guard let navigation else { return }

        if let progress = navigation.viewControllers
            .last(where: { ($0 as? FundingFlowScreenTagged)?.screen == .progress }) {
            navigation.popToViewController(progress, animated: true)
        } else {
            navigation.setViewControllers([makeScreen(.progress)], animated: true)
        }
    }

    /// Takes the flow down, then calls `completion`. A session that never
    /// started is dropped.
    func close(then completion: (() -> Void)? = nil) {
        guard let navigation else {
            didLeave()
            completion?()
            return
        }
        navigation.dismissIfPresented { [weak self] in
            self?.didLeave()
            completion?()
        }
    }

    func showHistory() {
        let onHistory = onHistory
        close { onHistory?() }
    }

    func startOver() {
        let direction = context.direction
        let onStartOver = onStartOver
        close { onStartOver?(direction) }
    }

    /// The app's toast over the screen on top.
    func showToast(_ message: String) {
        UIWindow.topWindow?.rootViewController?.topModalViewController.showToast(message: message, type: .success)
    }
}

private extension FundingFlowWireframe {
    func didLeave() {
        service.leave(context.entryId)
        onClose?()
        onClose = nil
    }

    func makeScreen(_ screen: FundingFlowScreen) -> UIViewController {
        switch screen {
        case .amount: FundingAmountViewFactory.createView(wireframe: self)
        case .summary: FundingSummaryViewFactory.createView(wireframe: self)
        case .fees: FundingFeesViewFactory.createView(wireframe: self)
        case .country: FundingCountryViewFactory.createView(wireframe: self)
        case .providers: FundingProvidersViewFactory.createView(wireframe: self)
        case .network: FundingNetworkViewFactory.createView(wireframe: self)
        case .token: FundingTokenViewFactory.createView(wireframe: self)
        case .deposit: FundingDepositViewFactory.createView(wireframe: self)
        case .progress: FundingProgressViewFactory.createView(wireframe: self)
        case .cancelConfirm: FundingCancelConfirmViewFactory.createView(wireframe: self)
        }
    }
}

/// The sheet a funding flow is shown in: presented like the app's other
/// bottom sheets, a rounded card between the safe area insets, with the
/// flow's screens pushed on it.
final class FundingFlowNavigationController: UINavigationController, ModalSheetPresenterDelegate {
    /// The user dragged the sheet away or tapped outside it.
    var onUserDismiss: (() -> Void)?

    init(screens: [UIViewController]) {
        super.init(nibName: nil, bundle: nil)
        setViewControllers(screens, animated: false)
        setNavigationBarHidden(true, animated: false)
        BottomSheetViewFacade.setupFullHeightSheet(from: self)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceContainer
    }

    /// Dismisses the sheet, or completes at once when it is not on screen.
    /// A provider's screen still over the sheet goes with it.
    func dismissIfPresented(completion: @escaping () -> Void) {
        guard let presenter = presentingViewController, !isBeingDismissed else {
            completion()
            return
        }
        presenter.dismiss(animated: true, completion: completion)
    }

    /// A drag moves the sheet only while the screen's list is scrolled to the
    /// top, so scrolling a list does not pull the sheet down.
    func presenterCanDrag(_: ModalPresenterProtocol) -> Bool {
        topViewController?.view.scrollViewsAtTop ?? true
    }

    func presenterDidHide(_: ModalPresenterProtocol) {
        onUserDismiss?()
    }
}
