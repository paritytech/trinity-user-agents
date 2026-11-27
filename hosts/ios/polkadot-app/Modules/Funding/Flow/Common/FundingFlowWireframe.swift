import Foundation
import SubstrateSdk
import TrUAPIHost
import UIKit
import UIKit_iOS

/// What a funding flow needs from the app around it.
@MainActor
protocol FundingFlowEnvironment: AnyObject {
    /// The payment balance's asset, as the app draws the digital dollar
    /// figure.
    var cashInfo: AssetBalanceDisplayInfo { get }
    /// The payment balance's symbol, set after its figure.
    var cashSymbol: String { get }
    var branding: FundingProviderBranding { get }
    /// Reads network and token icons from the providers' archives; nil
    /// when the app cannot, and the marks keep their monogram.
    var iconLoader: FundingIconLoader? { get }
    /// Calls `onChange` with what a payment can spend, now and on every
    /// balance change, until the calling task is cancelled. Returns without
    /// calling it when the balance cannot be followed.
    func followSpendable(_ onChange: @escaping @MainActor (Balance) -> Void) async
}

/// Moves between one funding flow's screens, on the sheet the flow is shown
/// in. Every screen's wireframe protocol is answered here, since they all
/// move on the same sheet.
@MainActor
final class FundingFlowWireframe {
    let flowState: FundingFlowState
    let service: FundingServiceProtocol
    let environment: FundingFlowEnvironment
    let viewModelFactory: FundingViewModelFactoryProtocol
    private(set) weak var navigation: FundingFlowNavigationController?

    init(
        flowState: FundingFlowState,
        service: FundingServiceProtocol,
        environment: FundingFlowEnvironment,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.flowState = flowState
        self.service = service
        self.environment = environment
        self.viewModelFactory = viewModelFactory
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

    /// Where to send a crypto deposit, over the session's progress, in place
    /// of every screen.
    func showDeposit() {
        navigation?.setViewControllers([makeScreen(.progress), makeScreen(.deposit)], animated: true)
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

    /// The history over the flow, which stays up underneath: closing the
    /// history comes back to the screen it was opened from.
    func showHistory() {
        flowState.onHistory?()
    }

    func startOver() {
        let direction = flowState.direction
        let onStartOver = flowState.onStartOver
        close { onStartOver?(direction) }
    }
}

extension FundingFlowWireframe: FundingToastPresentable {}

private extension FundingFlowWireframe {
    func didLeave() {
        service.leave(flowState.entryId)
        flowState.onClose?()
        flowState.onClose = nil
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
    /// The sheet went away without the flow closing it: the user dragged it
    /// away or tapped outside it, or something under it was dismissed.
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

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        if isBeingDismissed || presentingViewController == nil {
            onUserDismiss?()
        }
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
