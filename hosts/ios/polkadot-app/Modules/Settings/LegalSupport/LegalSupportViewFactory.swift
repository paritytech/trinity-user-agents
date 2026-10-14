import Foundation

@MainActor
enum LegalSupportViewFactory {
    static func createView() -> LegalSupportViewProtocol {
        let emailComposeAdapter = EmailComposeAdapter()
        let wireframe = LegalSupportWireframe(emailComposePresenter: emailComposeAdapter)
        let presenter = LegalSupportPresenter(wireframe: wireframe, emailComposePresenter: emailComposeAdapter)
        let view = LegalSupportViewController(presenter: presenter)

        presenter.view = view

        return view
    }
}
