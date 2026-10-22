import Foundation
import TrUAPIHost

/// The parts of a product room that the room screen asks its product for: the edit history of a message, who
/// reacted to it and when, and the call buttons (02-DESIGN §3.3.7, 07-LATER §11.2). Everything else of the room
/// draws with the views of the native chat from the messages and the declarations of the room.
@MainActor
final class ChatProductRoomPresenter {
    let chatId: Chat.Id
    let productId: ChatExtension.Id
    let roomId: String

    let room: ProductRoomInteracting
    /// Opens the outgoing call screen of a call that the user starts. The runtime of the app gives it once it runs.
    let outgoingCalls: () -> ProductOutgoingCalling?
    let logger: LoggerProtocol

    /// Closes the room while the app is in the background, and opens it again after (P6.9).
    var backgroundObserver: ChatRoomBackgroundObserver?

    init?(
        chatId: Chat.Id,
        room: ProductRoomInteracting,
        outgoingCalls: @escaping () -> ProductOutgoingCalling? = { RootDependencyLocator.getDependency() },
        logger: LoggerProtocol = Logger.shared
    ) {
        guard case let .chatExtension(productId, roomId?) = chatId else {
            return nil
        }

        self.chatId = chatId
        self.productId = productId
        self.roomId = roomId
        self.room = room
        self.outgoingCalls = outgoingCalls
        self.logger = logger
    }

    /// The id under which the product knows a message of the room.
    func productMessageId(_ messageId: Chat.MessageId) -> String {
        Chat.ProductMessageId.product(messageId, in: chatId)
    }

    func screenWillAppear() {
        backgroundObserver?.screenDidAppear()
    }

    func screenWillDisappear() {
        backgroundObserver?.screenDidDisappear()
    }

    /// A call button of the header: the outgoing call screen opens at once, as in the native chat, and the product
    /// starts the call under the id that the host chose (07-LATER §11.2).
    func requestCall(video: Bool) {
        guard let calls = outgoingCalls() else {
            logger.warning("Room \(roomId) has no call media")
            return
        }

        Task { [productId, roomId] in
            await calls.startOutgoingCall(productId: productId, roomId: roomId, video: video)
        }
    }

    /// Who reacted to a message with each emoji, and when, or nil when the product does not answer.
    func reactionDetails(of messageId: Chat.MessageId) async -> [ChatReactionDetail]? {
        do {
            let request = ProductChatReactionsQueryRequest(roomId: roomId, messageId: productMessageId(messageId))
            return try await room.reactions(request).reactions
        } catch {
            logger.warning("Room \(roomId) gave no reaction details: \(error)")
            return nil
        }
    }

    /// The edit history of a message, which the product keeps.
    func editHistoryInteractor(for message: Chat.LocalMessage) -> ProductEditHistoryInteractor {
        ProductEditHistoryInteractor(
            request: ProductChatEditHistoryQueryRequest(roomId: roomId, messageId: productMessageId(message.messageId)),
            status: message.status,
            room: room
        )
    }
}
