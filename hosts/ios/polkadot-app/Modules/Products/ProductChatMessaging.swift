import Foundation
import os
import Products
import AsyncExtensions

/// The chat calls a product's bot can make. All the rust chat bridge needs from
/// the native surface: `ChatHostBridge`'s fourth callback, `registerBot`, is
/// rejected outright.
protocol ProductChatMessaging: Sendable {
    func sendMessage(_ message: ProductBotMessage, roomId: String?) async throws -> String
    func createRoom(_ request: CreateRoomRequest) async throws -> CreateRoomResult
    func subscribeRooms() async throws -> AnyAsyncSequence<[RoomInfo]>
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

    func subscribeRooms() async throws -> AnyAsyncSequence<[RoomInfo]> {
        let (context, bot) = try requireMessaging()
        return await context.subscribeRooms(for: bot)
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
