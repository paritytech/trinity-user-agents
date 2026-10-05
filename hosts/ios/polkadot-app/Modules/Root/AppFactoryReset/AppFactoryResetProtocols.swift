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
        func navigateToFreshStart()
        func dismiss(from view: AppFactoryResetViewProtocol?)
    }
#endif
