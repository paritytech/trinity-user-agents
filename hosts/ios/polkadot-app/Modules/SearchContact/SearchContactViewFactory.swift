import Foundation

@MainActor
enum SearchContactViewFactory {
    static func createView(
        onChatFound: @escaping (ChatOpenModel) -> Void
    ) -> SearchContactViewProtocol? {
        guard let module = SearchContactModuleFactory.makeModule() else {
            return nil
        }

        let view = SearchContactViewController(presenter: module.presenter)
        module.presenter.view = view

        // Dismissal belongs to the presenting screen, not the wireframe, so the panel host can
        // reuse the same module without a modal to dismiss.
        module.wireframe.onChatFound = { [weak view] model in
            view?.dismiss(animated: true) { onChatFound(model) }
        }

        return view
    }
}
