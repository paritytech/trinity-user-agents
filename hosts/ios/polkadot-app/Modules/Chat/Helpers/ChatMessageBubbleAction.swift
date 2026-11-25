import Foundation

enum ChatMessageBubbleAction: Equatable {
    case reply
    case edit
    case copy
    case reaction
    case edited
    case delete
}

extension ChatMessageBubbleAction {
    /// In a product-source room, the user can delete a message that the user wrote.
    static func textMessageActions(
        for status: Chat.LocalMessage.Status,
        input: Chat.PeerMetadataInput,
        isEdited: Bool,
        isProductSource: Bool = false
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

        if status.isOutgoing, isProductSource {
            actions.append(.delete)
        }

        return actions
    }
}
