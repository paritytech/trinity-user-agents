import Foundation

@MainActor
enum FiatOnRampViewFactory {
    static func createView(
        context: WalletFlowContextProtocol
    ) -> FiatOnRampViewProtocol? {
        let wireframe = FiatOnRampWireframe(context: context)
        let interactor = FiatOnRampInteractor(
            fiatOnrampService: context.fiatOnrampService
        )
        let presenter = FiatOnRampPresenter(
            interactor: interactor,
            wireframe: wireframe
        )
        let view = FiatOnRampViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
