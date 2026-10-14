import UIKit

final class RootWireframe: RootWireframeProtocol {
    let window: UIWindow
    let animation = RootControllerAnimationCoordinator()
    private let userNotificationService: UserNotificationServicing
    private weak var foregroundVisibilityReporter: PushForegroundVisibilityReporting?
    private let deepLinkHandler: DeferredLinkHandling
    private let flowStateProvider: any SPAFlowStateProviding

    init(
        window: UIWindow,
        userNotificationService: UserNotificationServicing,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting?,
        deepLinkHandling: DeferredLinkHandling,
        flowStateProvider: any SPAFlowStateProviding
    ) {
        self.window = window
        self.userNotificationService = userNotificationService
        self.foregroundVisibilityReporter = foregroundVisibilityReporter
        deepLinkHandler = deepLinkHandling
        self.flowStateProvider = flowStateProvider
    }

    func showDashboard() {
        guard let dashboard = MainTabBarViewFactory.createView(
            userNotificationService: userNotificationService,
            foregroundVisibilityReporter: foregroundVisibilityReporter,
            deepLinkHandling: deepLinkHandler,
            flowStateProvider: flowStateProvider
        ) else {
            return
        }

        animation.animateTransition(to: dashboard.controller, in: window)
    }

    func showOnboarding(with observer: RootStateObserving) {
        guard let usernameView = ClaimUsernameViewFactory.createLiteClaimView(observer: observer) else {
            return
        }

        let viewContainer = AppNavigationController(rootViewController: usernameView.controller)

        animation.animateTransition(to: viewContainer, in: window)
    }

    func showRestoreFromCloud(with observer: RootStateObserving) {
        guard let restoreFromCloudView = RestoreFromCloudViewFactory.createView(with: observer) else {
            return
        }

        animation.animateTransition(to: restoreFromCloudView.controller, in: window)
    }

    func showUsernameClaim(with observer: RootStateObserving) {
        guard let usernameView = ClaimUsernameViewFactory.createLiteClaimView(observer: observer) else {
            return
        }

        let viewContainer = AppNavigationController(rootViewController: usernameView.controller)

        animation.animateTransition(to: viewContainer, in: window)
    }

    func showThemeSelection(with observer: RootStateObserving) {
        let view = ThemeSelectionViewFactory.createView(observer: observer)
        animation.animateTransition(to: view, in: window)
    }

    func showBroken() {
        // normally user must not see this but on malicious devices it is possible
        let brokenController = UIViewController()
        brokenController.view.backgroundColor = .red

        animation.animateTransition(to: brokenController, in: window)
    }

    func showUsernameCheck(with observer: RootStateObserving) {
        guard
            let destination = CheckUsernameViewFactory.createView(with: observer)
        else {
            return
        }

        animation.animateTransition(to: destination.controller, in: window)
    }
}

#if TESTNET_FEATURE
    extension RootWireframe {
        func showAppFactoryResetSheet() {
            guard let presenter = window.topmostViewController ?? window.rootViewController else {
                return
            }

            let sheetView = AppFactoryResetViewFactory.createView()
            presenter.present(sheetView.controller, animated: true)
        }
    }
#endif
