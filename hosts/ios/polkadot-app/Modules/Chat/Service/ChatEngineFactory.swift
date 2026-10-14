import Foundation
import MessageExchangeKit
import Products
import Keystore_iOS

protocol ChatEngineFactoryProtocol {
    func createChatEngine(for model: ChatOpenModel.NewRequest) throws -> ChatEngineProtocol
    func createChatEngine(for model: Chat.Id) throws -> ChatEngineProtocol
}

final class ChatEngineFactory: ChatEngineFactoryProtocol {
    let flowState: ChatFlowState
    let tldProvider: DotNsTldProviding

    init(
        flowState: ChatFlowState,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) {
        self.flowState = flowState
        self.tldProvider = tldProvider
    }

    func createChatEngine(for model: Chat.Id) throws -> any ChatEngineProtocol {
        try createChatEngine(for: nil, chatId: model)
    }

    func createChatEngine(for model: ChatOpenModel.NewRequest) throws -> any ChatEngineProtocol {
        let chatId = Chat.Id.person(model.remoteContact.accountId)
        return try createChatEngine(for: model, chatId: chatId)
    }

    private func createChatEngine(
        for request: ChatOpenModel.NewRequest?,
        chatId: Chat.Id
    ) throws -> any ChatEngineProtocol {
        let chatIdFactory = ChatPushIdFactory(
            encryptionManager: ChatEncryptionManager(),
            signManager: ChatSignerManager(),
            sessionIdFactory: PeerSessionIdFactory(),
            logger: Logger.shared
        )

        let storageFacade = UserDataStorageFacade.shared
        let messageExchangeModeProvider = try ChatMessageExchangeModeProvider(
            tld: tldProvider.currentTldOrError()
        )
        let leaveChatService = LeaveChatService(
            outboxService: flowState.outboxService,
            messageExchangeModeProvider: messageExchangeModeProvider
        )
        let blockUserService = BlockUserService()

        let requestContext = ChatRequestEngineContext(
            pendingRequest: request,
            chatRequestStoreService: ChatRequestStoreService(
                messageExchangeModeProvider: messageExchangeModeProvider,
                storageFacade: storageFacade,
                pushIdFactory: chatIdFactory,
                deviceEncryptionKeyManager: DeviceEncryptionKeyManager.shared
            ),
            messageExchangeModeProvider: messageExchangeModeProvider,
            tokenProvider: APNSTokenProviderFacade.sharedManager,
            storageFacade: storageFacade
        )

        return ChatEngine(
            chatId: chatId,
            chatRequestContext: requestContext,
            chatExtensionRegistry: flowState.extensionsRegistry,
            messageProviderFactory: ChatMessageDataProviderFactory(),
            chatProvider: ChatContactDataProviderFactory(),
            settings: SettingsManager.shared,
            leaveChatService: leaveChatService,
            blockUserService: blockUserService
        )
    }
}
