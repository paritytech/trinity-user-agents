import Foundation
import os
import Products
import AsyncExtensions

/// The chat calls a product's bot can make. All the rust chat bridge needs from
/// the native surface.
///
/// A call about a room that the product did not create throws `ChatRoomError.unknownRoom`.
protocol ProductChatMessaging: Sendable {
    func sendMessage(_ message: ProductBotMessage, roomId: String?) async throws -> String
    func createRoom(_ request: CreateRoomRequest) async throws -> CreateRoomResult
    func setRoomFooter(roomId: String, footer: Chat.RoomFooter) async throws
    func subscribeRooms() async throws -> AnyAsyncSequence<[RoomInfo]>
    /// Chat Extension v2 `set_room_preview`, as Swift domain values. The rust bridge calls it once the
    /// bindings of P2.10 give it the request.
    func setRoomPreview(roomId: String, preview: Chat.RoomPreview) async throws
    /// Chat Extension v2 `delete_room`.
    func deleteRoom(roomId: String) async throws
    /// Chat Extension v2 `set_room_metadata`, as Swift domain values.
    func setRoomMetadata(roomId: String, name: String, description: String?, avatar: Chat.RoomImage?) async throws
    /// What the product declared for a room. The core reads the configuration before each feed update.
    func roomDeclarations(roomId: String) async throws -> Chat.RoomDeclarations
    /// Chat Extension v2 `set_room_config`.
    func setRoomConfig(roomId: String, config: Chat.RoomConfig) async throws
    /// Chat Extension v2 `set_room_footer`, second version.
    func setRoomFooterConfig(roomId: String, footer: Chat.RoomFooterConfig) async throws
    /// Chat Extension v2 `set_room_toolbar`.
    func setRoomToolbar(roomId: String, actions: [Chat.RoomToolbarAction]) async throws
    /// Chat Extension v2 `set_room_suggestions`.
    func setRoomSuggestions(roomId: String, suggestions: [Chat.RoomSuggestion]) async throws
    /// Chat Extension v2 `set_room_commands`.
    func setRoomCommands(roomId: String, commands: [Chat.RoomCommand]) async throws
    /// Chat Extension v2 `set_room_activity`.
    func setRoomActivity(roomId: String, activity: Chat.RoomActivity?) async throws
    /// Chat Extension v2 `update_feed` for a room that the host stores. Throws `Chat.ProductFeedError` for a
    /// refused update.
    func applyFeedUpdate(roomId: String, update: Chat.ProductFeedUpdate) async throws
    /// The features that the product declared for a room (Chat Extension v2 additions).
    func roomFeatures(roomId: String) async throws -> Chat.RoomFeatures
    /// Chat Extension v2 additions `set_room_features`.
    func setRoomFeatures(roomId: String, features: Chat.RoomFeatures) async throws
    /// Chat Extension v2 additions `set_message_meta`. Throws `Chat.ProductFeedError.unknownMessage` for a
    /// message that a room of the host store does not hold.
    func setMessageMeta(roomId: String, messageId: String, meta: Chat.MessageMeta) async throws
    /// Chat Extension v2 additions `set_room_call`.
    func setRoomCall(roomId: String, call: Chat.RoomCall?) async throws
    /// The rooms of the product that the user muted or archived.
    func roomUserStates() async throws -> [Chat.RoomUserStateEntry]
    /// The bot ids of the personas that the product registered.
    func personaIds() async throws -> [String]
    /// Chat Extension v2 additions `register_bot`: keeps a persona that the product speaks as (07-LATER §5).
    func registerPersona(botId: String, name: String, icon: String?) async throws -> Chat.PersonaRegistration
}

/// Serves those calls from a chat binding. Conformers differ only in where the
/// binding lives — the shared worker's native api holds one for its whole
/// lifetime, the rust runtime holds its own.
protocol BoundProductChatMessaging: ProductChatMessaging {
    var currentMessaging: ProductsNativeApi.MessagingSupport? { get }

    /// Pacing applied before the write. The shared worker keeps the native bot's
    /// typing affordance; the rust path opts out because its callbacks are
    /// synchronous and hold a core dispatch thread for the whole delay.
    var messageDeliveryDelay: MessageDeliveryDelay { get }
}

extension BoundProductChatMessaging {
    var messageDeliveryDelay: MessageDeliveryDelay { .humanInteraction }

    func sendMessage(_ message: ProductBotMessage, roomId: String?) async throws -> String {
        let (context, bot) = try requireMessaging()
        let content = message.toChatMessageContent()

        let chatMessage: Chat.LocalMessage =
            if let roomId {
                try await context.sendNewMessage(
                    from: bot,
                    roomId: roomId,
                    newContent: content,
                    messageDeliveryDelay: messageDeliveryDelay
                )
            } else {
                try await context.sendNewMessage(
                    from: bot,
                    newContent: content,
                    messageDeliveryDelay: messageDeliveryDelay
                )
            }

        return chatMessage.messageId
    }

    func createRoom(_ request: CreateRoomRequest) async throws -> CreateRoomResult {
        let (context, bot) = try requireMessaging()

        let status = try await context.createRoom(
            for: bot,
            roomId: request.roomId,
            name: request.name,
            icon: request.icon
        )

        return CreateRoomResult(status: status)
    }

    func setRoomFooter(roomId: String, footer: Chat.RoomFooter) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomFooter(for: bot, roomId: roomId, footer: footer)
    }

    func subscribeRooms() async throws -> AnyAsyncSequence<[RoomInfo]> {
        let (context, bot) = try requireMessaging()
        return await context.subscribeRooms(for: bot)
    }

    func setRoomPreview(roomId: String, preview: Chat.RoomPreview) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomPreview(for: bot, roomId: roomId, preview: preview)
    }

    func deleteRoom(roomId: String) async throws {
        let (context, bot) = try requireMessaging()
        try await context.deleteRoom(for: bot, roomId: roomId)
    }

    func setRoomMetadata(roomId: String, name: String, description: String?, avatar: Chat.RoomImage?) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomMetadata(
            for: bot,
            roomId: roomId,
            name: name,
            description: description,
            avatar: avatar
        )
    }

    func roomDeclarations(roomId: String) async throws -> Chat.RoomDeclarations {
        let (context, bot) = try requireMessaging()
        return try await context.roomDeclarations(for: bot, roomId: roomId)
    }

    func setRoomConfig(roomId: String, config: Chat.RoomConfig) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomConfig(for: bot, roomId: roomId, config: config)
    }

    func setRoomFooterConfig(roomId: String, footer: Chat.RoomFooterConfig) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomFooterConfig(for: bot, roomId: roomId, footer: footer)
    }

    func setRoomToolbar(roomId: String, actions: [Chat.RoomToolbarAction]) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomToolbar(for: bot, roomId: roomId, actions: actions)
    }

    func setRoomSuggestions(roomId: String, suggestions: [Chat.RoomSuggestion]) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomSuggestions(for: bot, roomId: roomId, suggestions: suggestions)
    }

    func setRoomCommands(roomId: String, commands: [Chat.RoomCommand]) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomCommands(for: bot, roomId: roomId, commands: commands)
    }

    func setRoomActivity(roomId: String, activity: Chat.RoomActivity?) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomActivity(for: bot, roomId: roomId, activity: activity)
    }

    func applyFeedUpdate(roomId: String, update: Chat.ProductFeedUpdate) async throws {
        let (context, bot) = try requireMessaging()
        try await context.applyFeedUpdate(for: bot, roomId: roomId, update: update)
    }

    func roomFeatures(roomId: String) async throws -> Chat.RoomFeatures {
        let (context, bot) = try requireMessaging()
        return try await context.roomFeatures(for: bot, roomId: roomId)
    }

    func setRoomFeatures(roomId: String, features: Chat.RoomFeatures) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomFeatures(for: bot, roomId: roomId, features: features)
    }

    func setMessageMeta(roomId: String, messageId: String, meta: Chat.MessageMeta) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setMessageMeta(for: bot, roomId: roomId, messageId: messageId, meta: meta)
    }

    func setRoomCall(roomId: String, call: Chat.RoomCall?) async throws {
        let (context, bot) = try requireMessaging()
        try await context.setRoomCall(for: bot, roomId: roomId, call: call)
    }

    func roomUserStates() async throws -> [Chat.RoomUserStateEntry] {
        let (context, bot) = try requireMessaging()
        return try await context.roomUserStates(for: bot)
    }

    func personaIds() async throws -> [String] {
        let (context, bot) = try requireMessaging()
        return try await context.personaIds(for: bot)
    }

    func registerPersona(botId: String, name: String, icon: String?) async throws -> Chat.PersonaRegistration {
        let (context, bot) = try requireMessaging()
        return try await context.registerPersona(for: bot, botId: botId, name: name, icon: icon)
    }
}

private extension BoundProductChatMessaging {
    /// One read of the binding, so a rebind cannot tear the `context`/`bot` pair
    /// across two separate reads.
    func requireMessaging() throws -> (ChatExtensionDiscoverContextProtocol, any ChatExtensionBotProtocol) {
        let messaging = currentMessaging
        guard let context = messaging?.context else { throw ProductNativeApiError.messagesNotSupported }
        guard let bot = messaging?.bot else { throw ProductNativeApiError.chatBotMissing }
        return (context, bot)
    }
}

/// Holds the chat binding for a runtime that serves the core directly instead
/// of going through the shared worker's native api. Bound while the chat
/// surface is alive and cleared on dispose, matching
/// ``ProductsNativeApi/bindMessaging(_:)``.
/// The surface is one product's, and a product outlives any one runtime bound
/// to it: every emission of the product list builds fresh runtimes, and the one
/// that is kept is not always the one that was just built. A binding therefore
/// names its owner, and only that owner can clear it, so a runtime dropped
/// without ever starting cannot take the live one's chat down with it.
final class ProductChatSurface: BoundProductChatMessaging, @unchecked Sendable {
    let messageDeliveryDelay: MessageDeliveryDelay = .immediate

    private struct Binding {
        let support: ProductsNativeApi.MessagingSupport
        let owner: ObjectIdentifier
    }

    private let bound = OSAllocatedUnfairLock<Binding?>(initialState: nil)

    var currentMessaging: ProductsNativeApi.MessagingSupport? {
        bound.withLock { $0?.support }
    }

    func bind(_ support: ProductsNativeApi.MessagingSupport, owner: AnyObject) {
        bound.withLock { $0 = Binding(support: support, owner: ObjectIdentifier(owner)) }
    }

    /// Clears the binding only while `owner` still holds it.
    func unbind(owner: AnyObject) {
        let owner = ObjectIdentifier(owner)

        bound.withLock { binding in
            guard binding?.owner == owner else { return }

            binding = nil
        }
    }
}
