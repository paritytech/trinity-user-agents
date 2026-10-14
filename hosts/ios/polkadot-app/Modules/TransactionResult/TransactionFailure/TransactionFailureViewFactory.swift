import Foundation

@MainActor
enum TransactionFailureViewFactory {
    static func createView(
        for onRetry: TransactionFailureCompletion?
    ) -> TransactionFailureViewProtocol? {
        let wireframe = TransactionFailureWireframe(onHide: onRetry)

        let presenter = TransactionFailurePresenter(wireframe: wireframe)

        let view = TransactionFailureViewController(presenter: presenter)

        presenter.view = view

        return view
    }
}
