import Foundation
import SubstrateSdk
import ChainRegistry

@MainActor
enum WalletMainViewFactory {
    static func createView(
        with context: WalletFlowContextProtocol,
        chainAssetId: ChainAssetId
    ) -> WalletMainViewProtocol? {
        let chainRegistry = ChainRegistryFacade.sharedRegistry

        guard
            let chain = chainRegistry.getChain(for: chainAssetId.chainId),
            let chainAsset = chain.chainAsset(for: chainAssetId.assetId)
        else {
            return nil
        }
        let assetDetailsScene = AssetDetailsViewFactory.createEmbeddedScene(
            context: context,
            chainAsset: chainAsset
        )
        let wireframe = WalletMainWireframe(
            flowState: context.flowState,
            assetDetailsScene: assetDetailsScene
        )

        let interactor = WalletMainInteractor(
            pocketPrewarmer: PocketPrewarmer(
                products: context.flowState.productResolver,
                dotNsResolver: context.flowState.dotNsResolver
            ),
            pocket: .current,
            tldProvider: DotNsTldProviderFacade.shared,
            hostProvider: context.flowState.hostProvider,
            remoteConfig: FirebaseFacade.shared
        )

        let presenter = WalletMainPresenter(
            interactor: interactor,
            wireframe: wireframe
        )
        interactor.presenter = presenter

        let view = WalletMainViewController(
            presenter: presenter,
            assetDetailsScene: assetDetailsScene
        )
        presenter.view = view

        return view
    }
}
