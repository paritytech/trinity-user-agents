import Foundation
import os
import TrUAPIHost
import UIKit
import UIKitExt
@testable import polkadot_app

/// A product that answers the questions of a room screen from scripted answers and records what it received.
final class RecordingProductRoom: ProductRoomInteracting, @unchecked Sendable {
    struct Calls {
        var messageMenus: [ProductChatMessageActionsQueryRequest] = []
        var participants: [ProductChatParticipantsQueryRequest] = []
        var participantMenus: [ProductChatParticipantActionsQueryRequest] = []
        var reactions: [ProductChatReactionsQueryRequest] = []
        var events: [PublishedChatEvent] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: Calls())

    var messageMenuAnswer: Result<ProductChatMessageActionsQueryItem, Error> = .failure(
        ChatQueryError.refused(.notAvailable)
    )
    var participantsAnswer: (ChatParticipantsQuery) -> Result<ProductChatParticipantsQueryItem, Error> = { _ in
        .failure(ChatQueryError.refused(.notAvailable))
    }

    var participantMenuAnswer: Result<ProductChatParticipantActionsQueryItem, Error> = .failure(
        ChatQueryError.refused(.notAvailable)
    )
    var reactionsAnswer: Result<ProductChatReactionsQueryItem, Error> = .failure(
        ChatQueryError.refused(.notAvailable)
    )

    var calls: Calls {
        state.withLock { $0 }
    }

    var events: [ChatEvent] {
        calls.events.map(\.event)
    }

    func messageMenu(
        _ request: ProductChatMessageActionsQueryRequest
    ) async throws -> ProductChatMessageActionsQueryItem {
        state.withLock { $0.messageMenus.append(request) }
        return try messageMenuAnswer.get()
    }

    func participants(_ request: ProductChatParticipantsQueryRequest) async throws -> ProductChatParticipantsQueryItem {
        state.withLock { $0.participants.append(request) }
        return try participantsAnswer(request.query).get()
    }

    func participantMenu(
        _ request: ProductChatParticipantActionsQueryRequest
    ) async throws -> ProductChatParticipantActionsQueryItem {
        state.withLock { $0.participantMenus.append(request) }
        return try participantMenuAnswer.get()
    }

    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        state.withLock { $0.reactions.append(request) }
        return try reactionsAnswer.get()
    }

    func publish(_ event: ChatEvent, roomId: String) async throws {
        state.withLock { $0.events.append(PublishedChatEvent(roomId: roomId, peer: "native", event: event)) }
    }

    /// Waits until the product has `count` events, for an event that a task sends.
    func waitForEvents(_ count: Int) async {
        for _ in 0 ..< 200 where calls.events.count < count {
            try? await Task.sleep(for: .milliseconds(5))
        }
    }
}

/// Records the navigation of a product room. A confirmation runs at once when `confirms` is true.
@MainActor
final class RecordingProductRoomWireframe: ChatProductRoomWireframing {
    var confirms = true
    private(set) var confirmed: [URL] = []
    private(set) var opened: [URL] = []

    func confirmProductLink(_ url: URL, from _: ControllerBackedProtocol?, onOpen: @escaping () -> Void) {
        confirmed.append(url)
        if confirms {
            onOpen()
        }
    }

    func openProductLink(_ url: URL, from _: ControllerBackedProtocol?) {
        opened.append(url)
    }
}

/// A resolver that has no image.
struct NoImageResolver: ChatResourceResolving {
    func data(for _: polkadot_app.ChatResource) async -> Data? { nil }
    func image(for _: polkadot_app.ChatResource) async -> UIImage? { nil }
}

enum ProductRoomTestData {
    static let productId = "game.dot"
    static let chatId = Chat.Id.chatExtension(productId, roomId: "lobby")

    static func policy(_ config: Chat.RoomConfig) -> ChatRoomPolicy {
        ChatRoomPolicy(footer: nil, declarations: declarations(config: config))
    }

    static func declarations(
        config: Chat.RoomConfig = .hostDefault,
        toolbar: [Chat.RoomToolbarAction]? = nil,
        suggestions: [Chat.RoomSuggestion]? = nil,
        commands: [Chat.RoomCommand]? = nil,
        footer: Chat.RoomFooterConfig? = nil
    ) -> Chat.RoomDeclarations {
        Chat.RoomDeclarations(
            description: nil,
            avatar: nil,
            config: config,
            configGeneration: 1,
            preview: .hostDefault,
            toolbar: toolbar,
            suggestions: suggestions,
            commands: commands,
            footerConfig: footer
        )
    }

    static func config(
        messageActions: Bool = false,
        participants: Bool = false,
        participantActions: Bool = false,
        reactionDetails: Bool = false,
        productSource: Bool = false
    ) -> Chat.RoomConfig {
        Chat.RoomConfig(
            productSource: productSource,
            reactionDetails: reactionDetails,
            participants: participants,
            messageActions: messageActions,
            participantActions: participantActions,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: false
        )
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

    static func action(
        _ id: String,
        destructive: Bool = false,
        handler: Chat.ProductActionHandler = .callback
    ) -> Chat.ProductMenuAction {
        Chat.ProductMenuAction(actionId: id, label: id, icon: nil, destructive: destructive, handler: handler)
    }

    static func wireAction(
        _ id: String,
        destructive: Bool = false,
        handler: ChatActionHandler = .callback
    ) -> ChatProductAction {
        ChatProductAction(actionId: id, label: id, icon: nil, destructive: destructive, handler: handler)
    }
}
