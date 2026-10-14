import Foundation

@MainActor
enum LocalAuthViewFactory {
    static func createView(
        with authDismissable: AuthorizationDismissable,
        retriable: Bool
    ) -> LocalAuthViewProtocol? {
        let interactor = LocalAuthInteractor(deviceAuth: DeviceAuthentication())
        let wireframe = LocalAuthWireframe(authDismissable: authDismissable)

        let presenter = LocalAuthPresenter(
            interactor: interactor,
            wireframe: wireframe,
            retriable: retriable,
            logger: Logger.shared
        )

        let view = LocalAuthViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }

    static func createInPlaceView(
        with authDismissable: AuthorizationDismissable
    ) -> LocalAuthViewProtocol? {
        let interactor = LocalAuthInteractor(deviceAuth: DeviceAuthentication())
        let wireframe = LocalAuthWireframe(authDismissable: authDismissable)

        let presenter = LocalAuthPresenter(
            interactor: interactor,
            wireframe: wireframe,
            retriable: false,
            logger: Logger.shared
        )

        let view = LocalAuthTransparentViewController(presenter: presenter)
        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
