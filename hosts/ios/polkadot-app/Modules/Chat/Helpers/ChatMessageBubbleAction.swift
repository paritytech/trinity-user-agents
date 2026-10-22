import Foundation

enum ChatMessageBubbleAction: Equatable {
    case reply
    case edit
    case copy
    case reaction
    case edited
}

extension ChatMessageBubbleAction {
    static func textMessageActions(
        for status: Chat.LocalMessage.Status,
        input: Chat.PeerMetadataInput,
        isEdited: Bool
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

        return actions
    }
}
