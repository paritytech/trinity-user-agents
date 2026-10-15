import Foundation
import TrUAPIHost

// Maps the menus of the Chat Extension v2 queries to the Swift domain values of the room screen. As in
// `ChatExtensionV2+Domain.swift`, this file never names the resource type of the bindings.

extension Chat.ProductMenuAction {
    init(_ action: ChatProductAction) {
        let icon: Chat.ProductResource? =
            switch action.icon {
            case let .inline(mimeType, bytes): .inline(mimeType: mimeType, data: bytes)
            case let .preimage(key): .preimage(key: key)
            case nil: nil
            }

        self.init(
            actionId: action.actionId,
            label: action.label,
            icon: icon,
            destructive: action.destructive,
            handler: Chat.ProductActionHandler(action.handler)
        )
    }
}

extension Chat.ProductMessageMenu {
    init(_ menu: ChatMessageMenu) {
        switch menu {
        case let .custom(actions):
            self = .custom(actions.map(Chat.ProductMenuAction.init))
        case let .extendBuiltIn(actions, hidden):
            self = .extendBuiltIn(
                actions.map(Chat.ProductMenuAction.init),
                hidden: Set(hidden.map(ChatRoomPolicy.BuiltInMessageAction.init))
            )
        }
    }
}

extension Chat.ProductParticipantMenu {
    init(_ menu: ChatParticipantMenu) {
        switch menu {
        case let .custom(actions): self = .custom(actions.map(Chat.ProductMenuAction.init))
        case let .extendBuiltIn(actions): self = .extendBuiltIn(actions.map(Chat.ProductMenuAction.init))
        }
    }
}

extension ChatRoomPolicy.BuiltInMessageAction {
    init(_ action: ChatBuiltInMessageAction) {
        switch action {
        case .copy: self = .copy
        case .reply: self = .reply
        case .edit: self = .edit
        case .delete: self = .delete
        case .react: self = .react
        case .retry: self = .retry
        }
    }
}
