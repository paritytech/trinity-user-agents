import Foundation
import Operation_iOS

protocol ChatRoomUserStateRecording: Sendable {
    /// Keeps the mute and the archive state that the user set for a product room, and tells the product.
    func record(_ state: Chat.RoomUserState, for chatId: Chat.Id) async throws
}

enum ChatRoomUserStateError: Error, Equatable {
    /// Only a room of a product has a user state.
    case notProductRoom
}

/// Writes the user state of a product room into `CDChatRoomMetadata` and sends `UserStateChanged` to the product
/// (07-LATER §6.2). The mute and archive actions of P10.10 call it.
///
/// The store write comes first. When the event cannot go out, the recorder logs the error and does not throw. The
/// core reads the stored states again when the worker of the product subscribes, so the product gets the state then.
final class ChatRoomUserStateRecorder: ChatRoomUserStateRecording, @unchecked Sendable {
    private let storageFacade: StorageFacadeProtocol
    private let publisher: ChatRoomEventPublishing
    private let logger: LoggerProtocol

    init(storageFacade: StorageFacadeProtocol, publisher: ChatRoomEventPublishing, logger: LoggerProtocol) {
        self.storageFacade = storageFacade
        self.publisher = publisher
        self.logger = logger
    }

    func record(_ state: Chat.RoomUserState, for chatId: Chat.Id) async throws {
        guard case let .chatExtension(productId, roomId?) = chatId else {
            throw ChatRoomUserStateError.notProductRoom
        }

        let update = Chat.RoomUserStateUpdate(chatId: chatId, state: state)
        let repository = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatRoomDeclarationUpdateMapper<Chat.RoomUserStateUpdate>())
        )
        try await repository.saveOperation({ [update] }, { [] }).asyncExecute()

        do {
            try publisher.publish(.userStateChanged(state), productId: productId, roomId: roomId)
        } catch {
            logger.warning("[chat-user-state] \(productId)/\(roomId): the product gets the state later: \(error)")
        }
    }
}
