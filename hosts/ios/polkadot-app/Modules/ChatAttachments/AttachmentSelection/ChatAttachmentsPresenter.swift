import UIKit
import Foundation
import PolkadotUI
import UniformTypeIdentifiers
import Kingfisher

@MainActor
final class ChatAttachmentsPresenter {
    weak var view: ChatAttachmentsViewProtocol?
    let wireframe: ChatAttachmentsWireframeProtocol
    let interactor: ChatAttachmentsInteractorInputProtocol

    let onComplete: (ProcessedAttachmentResult) -> Void

    private var processedAttachments: [ProcessedAttachment]?

    init(
        interactor: ChatAttachmentsInteractorInputProtocol,
        wireframe: ChatAttachmentsWireframeProtocol,
        onComplete: @escaping (ProcessedAttachmentResult) -> Void
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.onComplete = onComplete
    }
}

private extension ChatAttachmentsPresenter {
    func provideViewModels() {
        guard let processedAttachments else {
            return
        }

        let viewModels: [AttachmentSelectionViewModel] = processedAttachments.compactMap { attachment in
            switch attachment.meta {
            case .image:
                let imageViewModel = LocalImageViewModel(
                    provider: LocalFileImageDataProvider(fileURL: attachment.fileUrl),
                    keepsCurrentImageWhileLoading: false
                )

                return AttachmentSelectionViewModel.image(imageViewModel)
            case .video:
                return AttachmentSelectionViewModel.video(attachment.fileUrl)
            default:
                return nil
            }
        }

        view?.didReceive(viewModels: viewModels)
    }
}

extension ChatAttachmentsPresenter: ChatAttachmentsPresenterProtocol {
    func setup() {
        interactor.setup()
    }

    func send(with text: String) {
        guard let processedAttachments else {
            return
        }

        let result = ProcessedAttachmentResult(
            message: text,
            attachments: processedAttachments
        )

        interactor.complete(rejectingAttachments: [])

        wireframe.dismiss(from: view) { [onComplete] in
            onComplete(result)
        }
    }

    func cancel() {
        interactor.complete(rejectingAttachments: processedAttachments ?? [])
        wireframe.dismiss(from: view, completion: {})
    }
}

extension ChatAttachmentsPresenter: ChatAttachmentsInteractorOutputProtocol {
    func didProcessAttachments(_ attachments: [ProcessedAttachment]) {
        processedAttachments = attachments
        provideViewModels()
    }
}
