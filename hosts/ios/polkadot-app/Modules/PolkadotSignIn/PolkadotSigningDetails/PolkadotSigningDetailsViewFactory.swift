import Foundation

@MainActor
enum PolkadotSigningDetailsViewFactory {
    static func createView(
        detailsText: String,
        isTransaction: Bool
    ) -> PolkadotSigningDetailsViewProtocol? {
        let wireframe = PolkadotSigningDetailsWireframe()
        let presenter = PolkadotSigningDetailsPresenter(
            wireframe: wireframe,
            detailsText: detailsText,
            isTransaction: isTransaction
        )
        let view = PolkadotSigningDetailsViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
