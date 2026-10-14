import DesignSystem
import UIKit
import UIKitExt

final class RootWindow: UIWindow {
    #if TESTNET_FEATURE
        private weak var issueReportController: UIViewController?
    #endif

    override init(windowScene: UIWindowScene) {
        super.init(windowScene: windowScene)

        applyThemeInterfaceStyle()

        registerForTraitChanges([DSThemeTrait.self]) { (window: RootWindow, _) in
            window.applyThemeInterfaceStyle()
        }

        #if TESTNET_FEATURE
            NotificationCenter.default.addObserver(
                self,
                selector: #selector(showIssueReport),
                name: UIApplication.userDidTakeScreenshotNotification,
                object: nil
            )
        #endif
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    #if TESTNET_FEATURE
        override func motionEnded(_ motion: UIEvent.EventSubtype, with _: UIEvent?) {
            guard motion == .motionShake else {
                return
            }
            showDebug()
        }

        func showDebug() {
            guard let debugView = DebugSettingsViewFactory.createView(
                flowStateProvider: SPAFlowStateProvider()
            ) else {
                return
            }
            let navigationController = AppNavigationController(rootViewController: debugView.controller)
            rootViewController?.present(navigationController, animated: true)
        }
    #endif
}

#if TESTNET_FEATURE
    @MainActor
    private extension RootWindow {
        /// Captures app content because screenshot notifications do not include the image.
        @objc func showIssueReport() {
            guard
                isKeyWindow,
                windowScene?.activationState == .foregroundActive,
                issueReportController == nil,
                let presenter = topmostViewController
            else { return }

            guard
                presenter.viewIfLoaded?.window === self,
                !presenter.isBeingPresented,
                !presenter.isBeingDismissed
            else { return }

            var captured = false
            let screenshot = UIGraphicsImageRenderer(bounds: bounds).image { _ in
                captured = drawHierarchy(in: bounds, afterScreenUpdates: false)
            }
            guard captured, let data = screenshot.pngData() else {
                let alert = UIAlertController(
                    title: String(localized: .reportIssueTitle),
                    message: String(localized: .reportIssueCaptureFailed),
                    preferredStyle: .alert
                )
                alert.addAction(UIAlertAction(title: String(localized: .reportIssueClose), style: .cancel))
                issueReportController = alert
                presenter.present(alert, animated: true)
                return
            }

            let controller = IssueReportViewFactory.createView(screenshot: screenshot, data: data)
            issueReportController = controller
            presenter.present(controller, animated: true)
        }
    }
#endif

private extension RootWindow {
    func applyThemeInterfaceStyle() {
        overrideUserInterfaceStyle = UIColor.bgSurfaceMain.isLight ? .light : .dark
    }
}
