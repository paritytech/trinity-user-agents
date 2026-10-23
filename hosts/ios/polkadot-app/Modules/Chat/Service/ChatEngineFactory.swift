import Foundation
import Keystore_iOS
import Products

protocol ChatEngineFactoryProtocol {
    func createChatEngine(for model: Chat.Id) throws -> ChatEngineProtocol
}

final class ChatEngineFactory: ChatEngineFactoryProtocol {
    let flowState: ChatFlowState

    init(flowState: ChatFlowState) {
        self.flowState = flowState
    }

    func createChatEngine(for chatId: Chat.Id) throws -> any ChatEngineProtocol {
        let productRoomServices: ProductRoomServices? =
            switch chatId {
            case .person:
                nil
            case let .chatExtension(extensionId, _):
                // The bot of the product can come after the room screen, so each call finds it then.
                ProductRoomServices(bot: RegistryProductRoom(
                    extensionId: extensionId,
                    registry: flowState.extensionsRegistry
                ))
            }

        return ChatEngine(
            chatId: chatId,
            chatExtensionRegistry: flowState.extensionsRegistry,
            messageProviderFactory: ChatMessageDataProviderFactory(),
            chatProvider: ChatContactDataProviderFactory(),
            settings: SettingsManager.shared,
            productRoomServices: productRoomServices
        )
    }
}
