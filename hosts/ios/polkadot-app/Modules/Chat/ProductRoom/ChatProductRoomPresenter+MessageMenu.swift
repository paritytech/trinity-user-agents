import Foundation
import TrUAPIHost
import UIKit

/// The message menu of a product room (P6.1, 02-DESIGN §6.6, scenarios S25 to S28).
extension ChatProductRoomPresenter {
    /// Builds the menu elements of a set of entries, with the images of the product icons by action id.
    typealias MessageMenuBuilder = (_ entries: [ChatMessageBubbleAction], _ icons: [String: UIImage]) -> [UIMenuElement]

    /// True when the room asks its product for the menu of each message (`message_actions`).
    var asksMessageMenus: Bool {
        policy?.productQueries.contains(.messageActions) == true
    }

    /// True when the host keeps the earlier versions of an edited message, so the host entry "View edit history" can
    /// show them. The host keeps no message of a product-source room, so that room has no such entry: its product can
    /// add an entry of its own.
    var offersHostEditHistory: Bool {
        policy?.messageStore != .product
    }

    /// The menu that the product answers for a message, or nil for the host default: the room does not ask, the
    /// product refuses, or the answer does not come in the lookup time.
    func messageMenu(for messageId: Chat.MessageId) async -> Chat.ProductMessageMenu? {
        guard asksMessageMenus, let message = messages[messageId] else {
            return nil
        }

        do {
            let item = try await room.messageMenu(ProductChatMessageActionsQueryRequest(
                roomId: roomId,
                message: message.productQueryMessage(in: chatId)
            ))
            return Chat.ProductMessageMenu(item.menu)
        } catch {
            logger.debug("Room \(roomId) uses the default menu: \(error)")
            return nil
        }
    }

    /// A menu element that asks the product for the menu when the menu opens. Until the answer, the menu shows a
    /// loading row. Without an answer, the menu shows the host entries.
    func deferredMessageMenu(
        messageId: Chat.MessageId,
        hostActions: [ChatMessageBubbleAction],
        build: @escaping MessageMenuBuilder
    ) -> UIMenuElement {
        UIDeferredMenuElement.uncached { [weak self] completion in
            guard let self else {
                completion(build(hostActions, [:]))
                return
            }

            Task { @MainActor in
                let answer = await self.messageMenu(for: messageId)
                let entries = ChatMessageBubbleAction.productMenu(hostActions: hostActions, answer: answer)
                let icons = await self.icons(of: entries.productActions)
                completion(build(entries, icons))
            }
        }
    }

    /// Runs a product entry of the menu of a message.
    func select(_ action: Chat.ProductMenuAction, messageId: Chat.MessageId) {
        perform(
            action.handler,
            event: .messageActionTriggered(messageId: productMessageId(messageId), actionId: action.actionId)
        )
    }
}

extension [ChatMessageBubbleAction] {
    /// The product entries, in their order.
    var productActions: [Chat.ProductMenuAction] {
        compactMap { entry in
            if case let .product(action) = entry { action } else { nil }
        }
    }
}
