import Foundation
import Products

struct ChatViewModelActions {
    let reply: (_ messageId: String) -> Void
    let copy: (_ content: String) -> Void
    let edit: (_ messageId: String) -> Void
    let toggleReaction: (_ messageId: String, _ emoji: String) -> Void
    let showReactionDetails: (_ messageId: String) -> Void
    let acceptChatRequest: () -> Void
    let declineChatRequest: () -> Void
    let showEditHistory: (_ messageId: String) -> Void
    let showFile: (URL) -> Void
    /// Asks the user before the app leaves for an external address.
    let openLink: (URL) -> Void
    let processAction: (_ action: Chat.Action) -> Void
    let selectAttachment: (Chat.LocalMessage.Content.Attachment) -> Void
    let unblockUser: () -> Void
    let startCall: (_ callType: ChatCallType) -> Void
    let openProduct: (ProductPage) -> Void
    /// Sends a failed message of a product-source room again.
    let retry: (_ messageId: String) -> Void
    /// Deletes a message after the user confirms.
    let delete: (_ messageId: String) -> Void
}
