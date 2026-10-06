#if TESTNET_FEATURE
    import UIKit
    import UIKitExt

    protocol AppFactoryResetViewProtocol: ControllerBackedProtocol {}

    @MainActor
    protocol AppFactoryResetPresenterProtocol: AnyObject {
        func actionStartOver()
        func actionDismiss()
    }

    protocol AppFactoryResetInteractorInputProtocol: AnyObject {
        func performReset()
    }

    @MainActor
    protocol AppFactoryResetInteractorOutputProtocol: AnyObject {
        func didCompleteReset()
        func didFailReset(_ error: Error)
    }

    @MainActor
    protocol AppFactoryResetWireframeProtocol: AnyObject {
        /// Releases the current screen hierarchy, so the dashboard's deinit stops its services before the wipe.
        func detachCurrentSession(from view: AppFactoryResetViewProtocol?)
        func navigateToFreshStart()
        func presentResetFailure(_ error: Error, retry: @escaping () -> Void)
        func dismiss(from view: AppFactoryResetViewProtocol?)
    }
#endif
