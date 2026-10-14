import Foundation
import MessageUI
import UIKitExt

/// Enumeration for possible errors during email composition
enum EmailError: Error {
    /// Indicates that the email service is not available on given device
    case serviceUnavailable
    /// Indicates that the email failed to send
    case sendingFailed
    /// Indicates an unknown error occurred within mail composer
    case unknownError
}

/// Enumeration for the result of email composition
enum EmailComposeResult {
    /// Indicates successful sending of the email or saving a draft
    case success
    /// Indicates the user cancelled the email composition
    case cancelled
    /// Indicates failure with associated error type
    case failure(EmailError)
}

struct EmailDraft {
    let subject: String
    let message: String
    let recipients: [String]
    let attachment: EmailAttachment?
}

struct EmailAttachment {
    let data: Data
    let mimeType: String
    let name: String
    let url: URL
}

/// Protocol that provides ability to compose emails using system Mail app
@MainActor
protocol EmailComposePresenting: AnyObject {
    /// Method dependency injection for presenter that will present share sheet
    /// - Parameter presenter: A view controller that conforms to ControllerBackedProtocol
    func use(presenter: ControllerBackedProtocol)
    /// Presents an email composition interface with the provided draft
    /// - Parameters:
    ///   - draft: The email draft containing subject, message, and recipients
    ///   - completion: The closure to call with the result after sending the email
    func presentEmail(with draft: EmailDraft, _ completion: @escaping (EmailComposeResult) -> Void)

    func canSendMail() -> Bool
}

final class EmailComposeAdapter: NSObject, EmailComposePresenting, MFMailComposeViewControllerDelegate {
    private weak var presenter: ControllerBackedProtocol!
    private let mailComposerClass: MFMailComposeViewController.Type
    private var completion: ((EmailComposeResult) -> Void)?
    init(
        mailComposerClass: MFMailComposeViewController.Type = MFMailComposeViewController.self
    ) {
        self.mailComposerClass = mailComposerClass
    }

    func use(presenter: ControllerBackedProtocol) {
        self.presenter = presenter
    }

    @MainActor
    func presentEmail(with draft: EmailDraft, _ completion: @escaping (EmailComposeResult) -> Void) {
        if !mailComposerClass.canSendMail() {
            completion(.failure(.serviceUnavailable))
            return
        }
        self.completion = completion

        let mailComposer = mailComposerClass.init()
        // This fixes tint navigation color issues for presented mail composer; .navigationBar.tintColor won't work
        mailComposer.modalPresentationStyle = .fullScreen
        mailComposer.mailComposeDelegate = self
        mailComposer.setSubject(draft.subject)
        mailComposer.setMessageBody(draft.message, isHTML: false)
        mailComposer.setToRecipients(draft.recipients)

        if let attachment = draft.attachment {
            mailComposer.addAttachmentData(
                attachment.data,
                mimeType: attachment.mimeType,
                fileName: attachment.name
            )
        }

        presenter.controller.present(mailComposer, animated: true, completion: nil)
    }

    func canSendMail() -> Bool {
        mailComposerClass.canSendMail()
    }

    func mailComposeController(
        _ controller: MFMailComposeViewController,
        didFinishWith result: MFMailComposeResult,
        error: Error?
    ) {
        controller.dismiss(animated: true) { [weak self] in
            self?.handle(result: result, error: error)
        }
    }
}

private extension EmailComposeAdapter {
    func handle(result: MFMailComposeResult, error: Error?) {
        if error != nil {
            completion?(.failure(.sendingFailed))
            return
        }

        switch result {
        case .cancelled:
            completion?(.cancelled)
        case .sent,
             .saved:
            completion?(.success)
        case .failed:
            completion?(.failure(.sendingFailed))
        @unknown default:
            completion?(.failure(.unknownError))
        }
    }
}
