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
            personDataStore: context.personDataStore,
            flowState: context.flowState
        )

        let networkStatusObserver = NetworkStatusObserver(
            networkStatusService: context.networkStatusService,
            chainIds: [chainAssetId.chainId],
            logger: Logger.shared
        )

        let interactor = WalletMainInteractor(
            collectiblesURLProvider: CollectiblesURLProvider(
                spaFlowState: context.flowState,
                dotNsLabel: AppConfig.DotNs.dotNsCollectibles,
                remoteConfig: FirebaseFacade.shared,
                firebaseFallback: { FirebaseApplicationService.shared.asyncWaitCollectiblesFallbackURL() }
            ),
            networkStatusObserver: networkStatusObserver,
            pocketPrewarmer: PocketPrewarmer(
                products: context.flowState.productResolver,
                dotNsResolver: context.flowState.dotNsResolver
            ),
            pocket: .current
        )

        let presenter = WalletMainPresenter(
            interactor: interactor,
            wireframe: wireframe,
            titleViewModelFactory: NetworkStatusTitleViewModelFactory(
                screenTitle: String(localized: .walletMainTitle)
            )
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
                wallet: wallet,
                personDataStore: context.personDataStore
            )
        )
        presenter.view = view

        return view
    }
}
