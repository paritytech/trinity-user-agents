import TrUAPIHost
import UIKit
import PolkadotUI
import Products
import UIKitExt

@MainActor
final class MainTabBarWireframe: MainTabBarWireframeProtocol {
    private let serviceCoordinator: ServiceCoordinatorProtocol
    private let tabFactory: TabFactoryProtocol
    private let moduleNavigator: ModuleNavigating
    private weak var presentedSettings: UINavigationController?
    /// Injected rather than read from the tab bar's view, which leaves the window under a full-screen modal.
    private weak var windowScene: UIWindowScene?
    private var paymentsSetupNotice: TopNoticeWindow?
    private var isPaymentsSetupNoticeDismissed = false
    private var funding: FundingScreens?

    init(
        serviceCoordinator: ServiceCoordinatorProtocol,
        tabFactory: TabFactoryProtocol,
        moduleNavigator: ModuleNavigating,
        windowScene: UIWindowScene?
    ) {
        self.serviceCoordinator = serviceCoordinator
        self.tabFactory = tabFactory
        self.moduleNavigator = moduleNavigator
        self.windowScene = windowScene
    }

    func showPolkadotSignIn(with url: URL, view: MainTabBarViewProtocol?) {
        guard let signInView = PolkadotSignInViewFactory.createView(
            serviceCoordinator: serviceCoordinator,
            url: url,
            onResult: { [weak self, weak view] result in
                self?.handleSignInResult(result, view: view)
            }
        ) else {
            return
        }
        view?.controller.topmostPresented.present(signInView.controller, animated: true)
    }

    func showPaymentsSetupNotice(isVisible: Bool) {
        guard isVisible else {
            paymentsSetupNotice?.hide()
            isPaymentsSetupNoticeDismissed = false
            return
        }
        guard !isPaymentsSetupNoticeDismissed, let windowScene else {
            return
        }

        let notice = paymentsSetupNotice ?? TopNoticeWindow(windowScene: windowScene)
        paymentsSetupNotice = notice
        notice.show(PaymentsSetupNoticeView { [weak self] in
            self?.isPaymentsSetupNoticeDismissed = true
            self?.paymentsSetupNotice?.hide()
        })
    }

    func openProduct(productId: ProductId) {
        ProductOpener(navigator: moduleNavigator).open(productId: productId)
    }

    func openChat(_ model: ChatOpenModel) {
        moduleNavigator.openChat(model)
    }

    func showConnectionStatus(_ viewModel: ConnectionStatusSheetViewModel, from view: MainTabBarViewProtocol?) {
        let sheet = ConnectionStatusSheetViewFactory.createView(viewModel: viewModel)
        view?.controller.topmostPresented.present(sheet, animated: true)
    }

    func showSettings(from view: MainTabBarViewProtocol?) {
        guard let settings = tabFactory.makeSettingsNavigation() else {
            return
        }
        presentedSettings = settings
        view?.controller.topmostPresented.present(settings, animated: true)
    }

    func showBackupSettings(from view: MainTabBarViewProtocol?) {
        guard
            let presenter = view?.controller.topmostPresented,
            let backup = BackupViewFactory.createView(serviceCoordinator: serviceCoordinator)
        else {
            return
        }

        if let settings = presentedSettings, settings.presentingViewController != nil {
            settings.pushViewController(backup.controller, animated: true)
            return
        }

        guard let settings = tabFactory.makeSettingsNavigation() else {
            return
        }
        presentedSettings = settings
        settings.pushViewController(backup.controller, animated: false)
        presenter.present(settings, animated: true)
    }
}

extension MainTabBarWireframe {
    func attachFunding(service: FundingServiceProtocol, view: MainTabBarViewProtocol?) {
        guard funding == nil else { return }

        let router = ProductsRouter()
        if let view { router.setPresentationView(view) }
        let screens = FundingScreens(
            service: service,
            environment: AppFundingEnvironment(router: router),
            notice: FundingFrameNoticePresenter(application: UIApplication.shared),
            logger: Logger.shared
        )
        funding = screens
        FundingActivityCenter.attach(service: service, router: screens.router)
    }

    func presentFunding(_ entry: FundingEntry) {
        funding?.present(entry)
    }

    func isShowingFunding(entryId: String) -> Bool {
        funding?.isShowing(entryId: entryId) ?? false
    }

    func presentProviderFrame(_ frame: FundingFrame) {
        funding?.present(frame)
    }

    func closeProviderFrames(entryId: String) {
        funding?.closeFrames(entryId: entryId)
    }

    func showProviderFrameNotice(
        for request: FundingFrameRequest,
        onShow: @escaping () -> Void,
        onDismiss: @escaping () -> Void
    ) {
        funding?.showFrameNotice(for: request, onShow: onShow, onDismiss: onDismiss)
    }

    func showProviderNotWorking(_ providerId: ProductId) {
        funding?.showProviderNotWorking(providerId)
    }
}

private extension MainTabBarWireframe {
    func handleSignInResult(_ result: PolkadotSignInResult, view: MainTabBarViewProtocol?) {
        switch result {
        case let .success(device):
            view?.controller.showToast(
                message: String(localized: .linkedDevicesSignInDeviceConnected),
                type: .success
            )
            showDeviceDetails(device, view: view)
        case let .noFreeSlots(message):
            let controller = NoSlotsAvailableViewFactory.createView(message: message)
            view?.controller.topmostPresented.present(controller, animated: true)
        case .failed:
            view?.controller.showToast(
                message: String(localized: .linkedDevicesSignInError),
                type: .error
            )
        }
    }

    func showDeviceDetails(_ device: Chat.LocalDevice, view: MainTabBarViewProtocol?) {
        guard
            let presenter = view?.controller.topmostPresented,
            let linkedDevicesView = LinkedDevicesViewFactory.createView(serviceCoordinator: serviceCoordinator),
            let detailsView = DeviceDetailsViewFactory.createView(
                device: device,
                serviceCoordinator: serviceCoordinator
            )
        else {
            return
        }

        if let settings = presentedSettings, settings === presenter {
            settings.pushViewController(linkedDevicesView.controller, animated: false)
            settings.pushViewController(detailsView.controller, animated: true)
            return
        }

        guard let settings = tabFactory.makeSettingsNavigation() else {
            return
        }
        presentedSettings = settings
        settings.pushViewController(linkedDevicesView.controller, animated: false)
        settings.pushViewController(detailsView.controller, animated: false)
        presenter.present(settings, animated: true)
    }
}
