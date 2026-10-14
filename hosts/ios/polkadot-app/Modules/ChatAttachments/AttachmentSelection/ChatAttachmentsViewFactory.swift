import Foundation
import Operation_iOS
import Keystore_iOS

@MainActor
enum ChatAttachmentsViewFactory {
    static func createView(
        providers: [ChatAttachmentProviding],
        flowState: ChatFlowState,
        onComplete: @escaping (ProcessedAttachmentResult) -> Void
    ) -> ChatAttachmentsViewProtocol? {
        guard let uploadStore = AttachmentStore.uploads() else {
            return nil
        }

        let interactor = ChatAttachmentsInteractor(
            providers: providers,
            uploadStore: uploadStore,
            audioSessionManager: flowState.audioSessionManager
        )

        let wireframe = ChatAttachmentsWireframe()

        let presenter = ChatAttachmentsPresenter(
            interactor: interactor,
            wireframe: wireframe,
            onComplete: onComplete
        )

        let view = ChatAttachmentsViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
