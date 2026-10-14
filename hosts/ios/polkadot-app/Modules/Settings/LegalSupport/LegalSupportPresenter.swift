import Foundation
import SafariServices
import UIKitExt

@MainActor
final class LegalSupportPresenter {
    weak var view: LegalSupportViewProtocol?
    let wireframe: LegalSupportWireframeProtocol
    let emailComposePresenter: EmailComposePresenting

    private var prewarmingToken: SFSafariViewController.PrewarmingToken?

    init(wireframe: LegalSupportWireframeProtocol, emailComposePresenter: EmailComposePresenting) {
        self.wireframe = wireframe
        self.emailComposePresenter = emailComposePresenter
    }

    deinit {
        prewarmingToken?.invalidate()
    }
}

// MARK: - Private functions

extension LegalSupportPresenter {
    private func url(for cell: LegalSupportViewModel.CellType) -> URL? {
        switch cell {
        case .termsOfUse: AppConfig.termsOfUseLink
        case .privacy: AppConfig.privacyPolicyLink
        case .contactUs: nil
        }
    }

    private func openMailApp() {
        if emailComposePresenter.canSendMail() {
            wireframe.openMailComposer(from: view)
        } else {
            wireframe.showContactEmailFallback(AppConfig.contactEmail, from: view)
        }
    }
}

// MARK: - LegalSupportPresenterProtocol

extension LegalSupportPresenter: LegalSupportPresenterProtocol {
    func setup() {
        prewarmingToken = wireframe.prewarmURLs([url(for: .termsOfUse)])
        view?.applyContent(LegalSupportViewModel.makeSections { [weak self] cell in
            self?.didTapCell(cell)
        })
    }

    func didTapCell(_ cell: LegalSupportViewModel.CellType) {
        switch cell {
        case .termsOfUse,
             .privacy:
            guard let url = url(for: cell), let view else { return }
            wireframe.showWeb(url: url, from: view, style: WebPresentableStyle(mode: .automatic))
        case .contactUs:
            openMailApp()
        }
    }
}
