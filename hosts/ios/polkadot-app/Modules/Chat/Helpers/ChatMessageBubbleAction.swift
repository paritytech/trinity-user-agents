import Foundation

enum ChatMessageBubbleAction: Equatable {
    case reply
    case edit
    case copy
    case reaction
    case edited
    /// Sends a failed message again.
    case retry
    /// Deletes the message after the user confirms. It is the last entry of the menu.
    case delete
    /// An entry that the product of the room defines (`ChatProductAction`).
    case product(Chat.ProductMenuAction)
}

extension ChatMessageBubbleAction {
    static func textMessageActions(
        for status: Chat.LocalMessage.Status,
        input: Chat.PeerMetadataInput,
        isEdited: Bool,
        offersRetry: Bool = false,
        offersDelete: Bool = false
    ) -> [ChatMessageBubbleAction] {
        var actions: [ChatMessageBubbleAction] = []

        if input.isInputField {
            actions.append(.reply)
            actions.append(.reaction)
        }

        actions.append(.copy)

        if status.isOutgoing {
            actions.append(.edit)
        }

        if isEdited {
            actions.append(.edited)
        }

        if offersRetry, status == .outgoing(.failed) {
            actions.append(.retry)
        }

        if offersDelete {
            actions.append(.delete)
        }

        return actions
    }
}
