import Foundation
import Foundation_iOS
import ChainRegistry

@MainActor
enum ContactsListViewFactory {
    static func createView(
        flowState: ChatFlowState
    ) -> ContactsListViewProtocol? {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let chainAssetId = AppConfig.Assets.mainAsset

        guard
            let chain = chainRegistry.getChain(for: chainAssetId.chainId),
            let chainAsset = chain.chainAsset(for: chainAssetId.assetId)
        else {
            return nil
        }

        let networkStatusObserver = NetworkStatusObserver(
            networkStatusService: flowState.networkStatusService,
            chainIds: [AppConfig.Chains.chatChain],
            logger: Logger.shared
        )

        let interactor = ContactsListInteractor(
            chatContactDataProviderFactory: ChatContactDataProviderFactory(),
            chatExtensionsRegistry: flowState.extensionsRegistry,
            networkStatusObserver: networkStatusObserver,
            foregroundVisibilityReporter: flowState.foregroundVisibilityReporter,
            logger: Logger.shared
        )

        let wireframe = ContactsListWireframe(flowState: flowState)

        let formatterFactory = AssetBalanceFormatterFactory()
        let tokenFormatter: (AssetBalanceDisplayInfo) -> TransferAmountViewModelFactoryProtocol = { info in
            TransferAmountViewModelFactory(
                targetAssetInfo: info,
                formatterFactory: formatterFactory
            )
        }

        let decoderFactory = ChatMessageDecoderFactory(extensionsRegistry: flowState.extensionsRegistry)

        let viewModelFactory = ContactsListViewModelFactory(
            chatMessageDecoderFactory: decoderFactory,
            chain: chain,
            tokenFormatter: tokenFormatter
        )

        let presenter = ContactsListPresenter(
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory,
            titleViewModelFactory: NetworkStatusTitleViewModelFactory(
                screenTitle: String(localized: .chatMainTitle)
            ),
            assetDisplayInfo: chainAsset.asset.digitalDollarDisplayInfo
        )

        let view = ContactsListViewController(
            presenter: presenter
        )

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
