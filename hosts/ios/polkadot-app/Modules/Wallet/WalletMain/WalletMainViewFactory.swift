import Foundation
import Keystore_iOS
import SubstrateSdk
import ChainRegistry

@MainActor
enum WalletMainViewFactory {
    static func createView(
        with context: WalletFlowContextProtocol,
        chainAssetId: ChainAssetId
    ) -> WalletMainViewProtocol? {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let walletRepo: WalletManagerRepositoryProtocol = .shared

        guard
            let wallet = try? walletRepo.main(),
            let chain = chainRegistry.getChain(for: chainAssetId.chainId),
            let chainAsset = chain.chainAsset(for: chainAssetId.assetId)
        else {
            return nil
        }
        let wireframe = WalletMainWireframe(
            flowState: context.flowState
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
            assetDetailsScene:
            AssetDetailsViewFactory.createEmbeddedScene(
                context: context,
                chainAsset: chainAsset
            ),
            identityDetailsScene:
            IdentityDetailsViewFactory.createEmbeddedScene(
                chainModel: chainAsset.chain,
                wallet: wallet
            )
        )
        presenter.view = view

        return view
    }
}
