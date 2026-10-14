import UIKit
import Foundation
import PolkadotUI
import UIKitExt

protocol ChatAttachmentsViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModels: [AttachmentSelectionViewModel])
}

@MainActor
protocol ChatAttachmentsPresenterProtocol: AnyObject {
    func setup()
    func send(with text: String)
    func cancel()
}

protocol ChatAttachmentsInteractorInputProtocol: AnyObject {
    func setup()
    func complete(rejectingAttachments: [ProcessedAttachment])
}

@MainActor
protocol ChatAttachmentsInteractorOutputProtocol: AnyObject {
    func didProcessAttachments(_ attachments: [ProcessedAttachment])
}

@MainActor
protocol ChatAttachmentsWireframeProtocol: AnyObject, AlertPresentable, ErrorPresentable {
    func dismiss(from view: ChatAttachmentsViewProtocol?, completion: @escaping () -> Void)
}
