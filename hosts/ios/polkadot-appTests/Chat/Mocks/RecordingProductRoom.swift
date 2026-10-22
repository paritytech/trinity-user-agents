import Foundation
import os
import TrUAPIHost
@testable import polkadot_app

/// A product that answers the questions of a room screen from scripted answers and records what it received.
final class RecordingProductRoom: ProductRoomInteracting, @unchecked Sendable {
    struct Calls {
        var reactions: [ProductChatReactionsQueryRequest] = []
        var editHistories: [ProductChatEditHistoryQueryRequest] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: Calls())

    var reactionsAnswer: Result<ProductChatReactionsQueryItem, Error> = .failure(
        ChatQueryError.refused(.notAvailable)
    )
    var editHistoryAnswer: Result<ProductChatEditHistoryQueryItem, Error> = .failure(
        ChatQueryError.refused(.notAvailable)
    )

    var calls: Calls {
        state.withLock { $0 }
    }

    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        state.withLock { $0.reactions.append(request) }
        return try reactionsAnswer.get()
    }

    func editHistory(_ request: ProductChatEditHistoryQueryRequest) async throws -> ProductChatEditHistoryQueryItem {
        state.withLock { $0.editHistories.append(request) }
        return try editHistoryAnswer.get()
    }
}

/// Records the room events that the app sends to products.
final class RecordingRoomEventPublisher: ChatRoomEventPublishing, @unchecked Sendable {
    struct Published: Equatable {
        let event: Chat.RoomEvent
        let productId: String
        let roomId: String
    }

    var error: Error?
    private let state = OSAllocatedUnfairLock<[Published]>(initialState: [])

    var events: [Published] {
        state.withLock { $0 }
    }

    func publish(_ event: Chat.RoomEvent, productId: ChatExtension.Id, roomId: String) throws {
        if let error { throw error }
        state.withLock { $0.append(Published(event: event, productId: productId, roomId: roomId)) }
    }
}

enum ProductRoomTestData {
    static let productId = "chat.dot"
    static let roomId = "dm-0101"
    static let chatId = Chat.Id.chatExtension(productId, roomId: roomId)
    static let account = Data(repeating: 1, count: 32)

    static func policy(_ config: Chat.RoomConfig) -> ChatRoomPolicy {
        ChatRoomPolicy(footer: nil, declarations: declarations(config: config))
    }

    static func declarations(
        config: Chat.RoomConfig = .hostDefault,
        contact: Chat.RoomContact? = nil
    ) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(
            config: config,
            configGeneration: 1,
            preview: .hostDefault,
            contact: contact
        )
    }

    static func config(
        reactionDetails: Bool = false,
        productSource: Bool = false,
        reportSeen: Bool = false
    ) -> Chat.RoomConfig {
        Chat.RoomConfig(productSource: productSource, reactionDetails: reactionDetails, reportSeen: reportSeen)
    }

    static func contact(
        _ state: Chat.RoomContact.State,
        username: String? = "alice"
    ) -> Chat.RoomContact {
        Chat.RoomContact(account: account, username: username, state: state)
    }

    static func message(
        _ id: String,
        content: Chat.LocalMessage.Content = .text("hello"),
        origin: Chat.LocalMessage.Origin = .chatExtension(productId),
        status: Chat.LocalMessage.Status = .incoming(.seen),
        reactions: [Chat.MessageReaction] = []
    ) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: id,
            chatId: chatId,
            origin: origin,
            creationSource: .localDevice,
            status: status,
            timestamp: 1_767_225_600_000,
            content: content,
            reactions: reactions,
            compactionId: nil,
            relatedMessages: []
        )
    }
}
