import Foundation
import Keystore_iOS
import SubstrateSdk
import Coinage
import ChainRegistry
import BackgroundExecution

@MainActor
enum AssetDetailsViewFactory {
    static func createEmbeddedScene(
        context: WalletFlowContextProtocol,
        chainAsset: ChainAsset
    ) -> AssetDetailsScene {
        let scene = createScene(context: context, chainAsset: chainAsset)
        scene.setup()

        return scene
    }

    private static func createScene(
        context: WalletFlowContextProtocol,
        chainAsset: ChainAsset
    ) -> AssetDetailsScene {
        let fundingDomainProvider = FundingDomainProvider(hostProvider: context.flowState.hostProvider)
        let interactor = AssetDetailsInteractor(
            priceLocalSubscriptionFactory: PriceProviderFactory.shared,
            fiatOnrampTrackingService: context.fiatOnrampTrackingService,
            chainAsset: chainAsset,
            coinageService: context.coinageService,
            coinageBackupSyncService: context.coinageBackupSyncService,
            fundingDomainProvider: fundingDomainProvider
        )

        #if TESTNET_FEATURE
            interactor.backgroundExecutor = ConnectionRetainingExecutor(
                provider: ChainRegistryFacade.sharedRegistry
            )
            interactor.topupService = TopUpService.create(for: chainAsset.chainAssetId)
        #endif

        let wireframe = AssetDetailsWireframe(context: context, fundingDomainProvider: fundingDomainProvider)
        let presenter = AssetDetailsPresenter(
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: WalletCardViewModelFactory(),
            logger: Logger.shared,
            chainAsset: chainAsset
        )
        let viewModel = AssetDetailsViewModel()
        let binding = AssetDetailsViewBinding(viewModel: viewModel)

        binding.bind(to: presenter)
        presenter.view = binding
        interactor.presenter = presenter

        return AssetDetailsScene(
            viewModel: viewModel,
            binding: binding,
            presenter: presenter,
            interactor: interactor
        )
    }
}
