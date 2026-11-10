import Foundation
import SubstrateSdk
import Keystore_iOS
import Operation_iOS
import MessageExchangeKit
import UIKitExt
import ChainRegistry

@MainActor
enum ChatViewFactory {
    static func createChatView(
        with openModel: ChatOpenModel,
        flowState: ChatFlowState
    ) -> ChatViewProtocol? {
        switch openModel {
        case let .existingChat(chatId):
            createExistingChatView(chatId: chatId, flowState: flowState)
        case let .newRequest(request):
            createPendingContactView(remoteRequest: request, flowState: flowState)
        }
    }

    private static func createExistingChatView(
        chatId: Chat.Id,
        flowState: ChatFlowState
    ) -> ChatViewProtocol? {
        createView(
            chatId: chatId,
            flowState: flowState,
            pendingRequest: nil
        )
    }

    private static func createPendingContactView(
        remoteRequest: ChatOpenModel.NewRequest,
        flowState: ChatFlowState
    ) -> ChatViewProtocol? {
        createView(
            chatId: .person(remoteRequest.remoteContact.accountId),
            flowState: flowState,
            pendingRequest: remoteRequest
        )
    }

    private static func createView(
        chatId: Chat.Id,
        flowState: ChatFlowState,
        pendingRequest: ChatOpenModel.NewRequest?
    ) -> ChatViewProtocol? {
        // at this time we should already have chains and assets
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        guard
            let chain = chainRegistry.getChain(for: AppConfig.Assets.mainAsset.chainId),
            let asset = chain.chainAsset(for: AppConfig.Assets.mainAsset.assetId),
            let uploadAttachmentStore = AttachmentStore.uploads(),
            let downloadAttachmentStore = AttachmentStore.dowloads(),
            let interactor = createInteractor(
                chatId: chatId,
                flowState: flowState,
                pendingRequest: pendingRequest,
                uploadsAttachmentStore: uploadAttachmentStore
            )
        else {
            return nil
        }

        let documentAdapter = DocumentPreviewAdapter()
        let videoPreviewPlayerFactory = VideoPreviewPlayerFactory(
            audioSessionManager: flowState.audioSessionManager
        )
        let wireframe = ChatWireframe(
            chainAsset: asset,
            flowState: flowState,
            documentAdapter: documentAdapter,
            uploadStore: uploadAttachmentStore,
            downloadStore: downloadAttachmentStore,
            videoPreviewPlayerFactory: videoPreviewPlayerFactory
        )

        let decoderFactory = ChatMessageDecoderFactory(extensionsRegistry: flowState.extensionsRegistry)
        let messageDecoders = decoderFactory.makeDecoders(for: chain, chatId: chatId)
        let amountFormatter = PlainTransferAmountViewModelFactory(
            targetAssetInfo: asset.asset.digitalDollarDisplayInfo,
            formatterFactory: AssetBalanceFormatterFactory()
        )
        let viewModelFactory = ChatViewModelFactory(
            balanceFactory: amountFormatter,
            customDecoders: messageDecoders,
            attachmentViewModelFactory: ChatAttachmentViewModelFactory(
                uploadAttachmentStore: uploadAttachmentStore,
                attachmentUploadStateProvider: flowState.attachmentUploadStateProvider,
                downloadAttachmentStore: downloadAttachmentStore,
                attachmentDownloadStateProvider: flowState.attachmentDownloadStateProvider
            ),
            productRepository: ProductRepositoryFactory().createRepository(),
            productNameCache: ProductNameCache(),
            flowState: flowState.flowState
        )

        let presenter = ChatPresenter(
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: viewModelFactory,
            moduleNavigator: ModuleNavigator()
        )

        let view = ChatViewController(presenter: presenter)

        documentAdapter.use(presenter: view)
        presenter.view = view
        interactor.presenter = presenter

        flowState.extensionsRegistry.getExtensions(for: chatId).forEach { chatExtension in
            chatExtension.attach(presentationView: view)
        }

        return view
    }

    private static func createInteractor(
        chatId: Chat.Id,
        flowState: ChatFlowState,
        pendingRequest: ChatOpenModel.NewRequest?,
        uploadsAttachmentStore _: AttachmentStoring
    ) -> ChatInteractor? {
        let storageFacade = UserDataStorageFacade.shared
        let reactionRepository = ChatReactionRepository(
            repository: AnyDataProviderRepository(
                storageFacade.createRepository(
                    filter: .reactionsFor(chatId: chatId.rawRepresentation),
                    sortDescriptors: [],
                    mapper: AnyCoreDataMapper(ChatMessageReactionMapper())
                )
            )
        )

        let engineFactory: ChatEngineFactoryProtocol = ChatEngineFactory(flowState: flowState)
        let chatEngine: any ChatEngineProtocol
        do {
            if let pendingRequest {
                chatEngine = try engineFactory.createChatEngine(for: pendingRequest)
            } else {
                chatEngine = try engineFactory.createChatEngine(for: chatId)
            }
        } catch {
            return nil
        }

        return ChatInteractor(
            chatId: chatId,
            engine: chatEngine,
            reactionRepository: reactionRepository,
            foregroundVisibilityReporter: flowState.foregroundVisibilityReporter,
            notificationsCleaner: flowState.notificationsCleaner
        )
    }
}
