import Foundation
import Products
import SubstrateSdk
import ChainRegistry

@MainActor
enum TopUpAcknowledgementViewFactory {
    static func createMismatchView(
        productId: ProductId,
        claimedAmount: Balance,
        requestedAmount: Balance
    ) -> TopUpMismatchViewProtocol? {
        guard let viewModelFactory = makeViewModelFactory() else {
            return nil
        }

        let wireframe = TopUpMismatchWireframe()
        let presenter = TopUpMismatchPresenter(
            wireframe: wireframe,
            productId: productId,
            claimedAmount: claimedAmount,
            requestedAmount: requestedAmount,
            viewModelFactory: viewModelFactory
        )
        let view = TopUpMismatchViewController(presenter: presenter)
        presenter.view = view

        BottomSheetViewFacade.setupNonNavigatingSheet(from: view, preferredHeight: nil)
        return view
    }

    static func createErrorView(productId: ProductId) -> TopUpErrorViewProtocol? {
        guard let viewModelFactory = makeViewModelFactory() else {
            return nil
        }

        let wireframe = TopUpErrorWireframe()
        let presenter = TopUpErrorPresenter(
            wireframe: wireframe,
            title: viewModelFactory.errorTitle(productId: productId),
            message: viewModelFactory.errorMessage(),
            closeButtonTitle: String(localized: .Common.close)
        )
        let view = TopUpErrorViewController(presenter: presenter)
        presenter.view = view

        BottomSheetViewFacade.setupNonNavigatingSheet(from: view, preferredHeight: nil)
        return view
    }

    private static func makeViewModelFactory() -> TopUpAcknowledgementViewModelMaking? {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let chainAssetId = AppConfig.Assets.mainAsset

        guard
            let chain = chainRegistry.getChain(for: chainAssetId.chainId),
            let chainAsset = chain.chainAsset(for: chainAssetId.assetId)
        else {
            return nil
        }

        return TopUpAcknowledgementViewModelFactory(chainAsset: chainAsset)
    }
}
