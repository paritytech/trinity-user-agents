import Foundation
import SubstrateSdk
import MessageExchangeKit

@MainActor
enum ChatCallViewFactory {
    @MainActor
    static func createView(
        peer: CallPeer,
        engine: CallEngineProtocol,
        role: CallRole,
        callType: ChatCallType
    ) -> ChatCallViewProtocol? {
        let interactor = ChatCallInteractor(
            callEngine: engine,
            audioSessionManager: CallAudioSessionManager.shared,
            backgroundTaskManager: CallBackgroundTaskManager(application: .shared),
            operatingSystemMediator: OperatingSystemMediator(),
            permissionsService: CallPermissionsService(),
            role: role,
            peer: peer,
            callType: callType,
            logger: Logger.shared
        )

        let wireframe = ChatCallWireframe()

        let presenter = ChatCallPresenter(
            peer: peer,
            role: role,
            callType: callType,
            interactor: interactor,
            wireframe: wireframe
        )

        let view = ChatCallViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
