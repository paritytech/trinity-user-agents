import Foundation
import UIKit

/// What the entries of a message menu act on.
struct ChatMessageMenuContent {
    /// The text that Copy copies.
    let text: String
    let actions: ChatViewModelActions
}

extension ChatViewModelFactory {
    /// The menu of a message. In a room that asks its product for message menus, the menu asks when it opens
    /// (P6.1); else it shows `hostActions`.
    func makeMessageMenu(
        messageId: String,
        hostActions: [ChatMessageBubbleAction],
        content: ChatMessageMenuContent
    ) -> () -> [UIMenuElement] {
        { [weak productRoom] in
            MainActor.assumeIsolated {
                let build: ChatProductRoomPresenter.MessageMenuBuilder = { [weak productRoom] entries, icons in
                    Self.menuElements(
                        entries,
                        messageId: messageId,
                        content: content,
                        icons: icons,
                        productRoom: productRoom
                    )
                }

                guard let productRoom, productRoom.asksMessageMenus else {
                    return build(hostActions, [:])
                }

                return [productRoom.deferredMessageMenu(messageId: messageId, hostActions: hostActions, build: build)]
            }
        }
    }

    /// The host entries, then the product entries in a section of their own (02-DESIGN §6.6).
    @MainActor
    static func menuElements(
        _ entries: [ChatMessageBubbleAction],
        messageId: String,
        content: ChatMessageMenuContent,
        icons: [String: UIImage],
        productRoom: ChatProductRoomPresenter?
    ) -> [UIMenuElement] {
        let actions = content.actions
        let hostElements: [UIMenuElement] = entries.compactMap { entry in
            switch entry {
            case .reply: UIAction.chatReply { actions.reply(messageId) }
            case .edit: UIAction.chatEdit { actions.edit(messageId) }
            case .copy: UIAction.chatCopy { actions.copy(content.text) }
            case .reaction: nil
            case .edited: UIAction.chatEditHistory { actions.showEditHistory(messageId) }
            case .retry: UIAction.chatRetry { actions.retry(messageId) }
            case .delete: UIAction.chatDelete { actions.delete(messageId) }
            case .product: nil
            case .readAloud: UIAction.chatReadAloud { [weak productRoom] in
                    productRoom?.readAloud(content.text, messageId: messageId)
                }
            }
        }

        let productElements: [UIMenuElement] = entries.productActions.map { action in
            UIAction.chatProduct(action, image: icons[action.actionId]) { [weak productRoom] in
                productRoom?.select(action, messageId: messageId)
            }
        }

        guard !hostElements.isEmpty, !productElements.isEmpty else {
            return hostElements + productElements
        }

        return hostElements + [UIMenu(options: .displayInline, children: productElements)]
    }
}

extension UIAction {
    /// The built-in "Read aloud" of a room that declares `read_aloud`.
    static func chatReadAloud(_ handler: @escaping () -> Void) -> UIAction {
        UIAction(
            title: String(localized: .ChatExtension.readAloud),
            image: UIImage(systemName: "speaker.wave.2")?.withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
        ) { _ in
            handler()
        }
    }

    /// A product entry of a menu. A destructive entry draws in red.
    static func chatProduct(
        _ action: Chat.ProductMenuAction,
        image: UIImage?,
        handler: @escaping () -> Void
    ) -> UIAction {
        UIAction(
            title: action.label,
            image: image,
            attributes: action.destructive ? .destructive : []
        ) { _ in
            handler()
        }
    }
}
