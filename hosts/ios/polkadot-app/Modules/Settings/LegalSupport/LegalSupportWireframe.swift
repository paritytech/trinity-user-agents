import Foundation
import UIKit
import UIKitExt

@MainActor
final class LegalSupportWireframe: LegalSupportWireframeProtocol {
    private let emailComposePresenter: EmailComposePresenting

    init(emailComposePresenter: EmailComposePresenting) {
        self.emailComposePresenter = emailComposePresenter
    }

    func openMailComposer(from view: (any LegalSupportViewProtocol)?) {
        guard let view else { return }
        emailComposePresenter.use(presenter: view)
        let draft = EmailDraft(
            subject: "",
            message: "",
            recipients: [AppConfig.contactEmail],
            attachment: nil
        )
        emailComposePresenter.presentEmail(with: draft) { _ in }
    }

    func showContactEmailFallback(_ email: String, from view: (any LegalSupportViewProtocol)?) {
        let copyAction = AlertPresentableAction(title: String(localized: .Common.copyEmail)) {
            UIPasteboard.general.string = email
        }

        let viewModel = AlertPresentableViewModel(
            title: String(localized: .Common.errorMailAppNotAvailable),
            message: email,
            actions: [copyAction],
            closeActionTitle: String(localized: .Common.close)
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }
}
