import Foundation
import Foundation_iOS
import ChainRegistry

@MainActor
enum ContactsListViewFactory {
    static func createView(
        flowState: ChatFlowState,
        scope: ChatListScope = .main
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
            scope: scope,
            userStates: RootDependencyLocator.getDependency(),
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
            productIconViewModelFactory: flowState.flowState.iconViewModelFactory,
            chain: chain,
            tokenFormatter: tokenFormatter
        )

        let presenter = ContactsListPresenter(
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory,
            titleViewModelFactory: NetworkStatusTitleViewModelFactory(
                screenTitle: scope == .main ? String(localized: .chatMainTitle)
                    : String(localized: .ChatExtension.listArchived)
            ),
            assetDisplayInfo: chainAsset.asset.digitalDollarDisplayInfo,
            scope: scope
        )

        let view = ContactsListViewController(
            presenter: presenter,
            scope: scope
        )

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
