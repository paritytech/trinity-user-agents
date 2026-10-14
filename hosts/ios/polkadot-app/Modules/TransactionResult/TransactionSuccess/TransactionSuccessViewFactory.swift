import Foundation

@MainActor
enum TransactionSuccessViewFactory {
    static func create(
        onDone: TransactionSuccessCompletion?
    ) -> TransactionSuccessViewProtocol? {
        let wireframe = TransactionSuccessWireframe(onHide: onDone)

        let presenter = TransactionSuccessPresenter(wireframe: wireframe)

        let view = TransactionSuccessViewController(
            presenter: presenter
        )

        presenter.view = view

        return view
    }
}
