import Foundation

@MainActor
enum FiatOnRampProviderViewFactory {
    static func createView(
        context: WalletFlowContextProtocol,
        amount: Decimal,
        purchaseLimit: FiatOnrampFiatPurchaseLimit?
    ) -> FiatOnRampProviderViewProtocol? {
        let wireframe = FiatOnRampProviderWireframe()
        let interactor = FiatOnRampProviderInteractor(
            fiatOnrampService: context.fiatOnrampService,
            fiatOnrampTrackingService: context.fiatOnrampTrackingService,
            amount: amount,
            purchaseLimit: purchaseLimit
        )
        let presenter = FiatOnRampProviderPresenter(
            interactor: interactor,
            wireframe: wireframe
        )
        let view = FiatOnRampProviderViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
