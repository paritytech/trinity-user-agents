import Foundation
import TrUAPIHost

// Maps the Chat Extension v2 values of the bindings to the Swift domain values of the chat store, and back
// where the core reads a value. The chat store and the notification extension do not link the bindings.
//
// In this module the name `ChatResource` is the resource type of the app, and the bindings module has an enum
// with the name of the module. So this file never names the resource type of the bindings, and reads each resource
// from the value that holds it.

extension Chat.RoomConfig {
    init(_ config: ChatRoomConfig) {
        self.init(
            productSource: config.productSource,
            reactionDetails: config.reactionDetails,
            participants: config.participants,
            messageActions: config.messageActions,
            participantActions: config.participantActions,
            allowedReactions: config.allowedReactions,
            placeholder: config.placeholder,
            reportSeen: config.reportSeen
        )
    }

    var wireValue: ChatRoomConfig {
        ChatRoomConfig(
            productSource: productSource,
            reactionDetails: reactionDetails,
            participants: participants,
            messageActions: messageActions,
            participantActions: participantActions,
            allowedReactions: allowedReactions,
            placeholder: placeholder,
            reportSeen: reportSeen
        )
    }
}

extension Chat.ProductResource {
    /// The store keeps no media type for a room image. The resolver reads the type from the bytes.
    var roomImage: Chat.RoomImage {
        switch self {
        case let .inline(_, data): .inline(data)
        case let .preimage(key): .preimage(key: key)
        }
    }
}

extension ChatRoomMetadata {
    var avatarImage: Chat.RoomImage? {
        switch avatar {
        case let .inline(mimeType, bytes): Chat.ProductResource.inline(mimeType: mimeType, data: bytes).roomImage
        case let .preimage(key): .preimage(key: key)
        case .sealed,
             nil: nil
        }
    }
}

extension Chat.RoomPreview {
    init(_ preview: ChatRoomPreview) {
        self.init(
            order: preview.order == .pinToTop ? .pinToTop : .byTimestamp,
            timestamp: preview.timestamp.map { Int64(clamping: $0) },
            badge: preview.badge.map(Chat.RoomBadge.init) ?? .hostDefault,
            text: preview.text?.text,
            ongoing: preview.text?.ongoing ?? false
        )
    }
}

extension Chat.RoomBadge {
    init(_ badge: ChatRoomBadge) {
        switch badge {
        case .hidden: self = .none
        case let .unreadCount(count): self = .unreadCount(Int32(clamping: count))
        case let .image(.inline(mimeType, bytes)):
            self = .image(Chat.ProductResource.inline(mimeType: mimeType, data: bytes).roomImage)
        case let .image(.preimage(key)):
            self = .image(.preimage(key: key))
        case .image(.sealed):
            self = .none
        }
    }
}

extension Chat.ProductActionHandler {
    init(_ handler: ChatActionHandler) {
        switch handler {
        case let .deeplink(url): self = .deeplink(url: url)
        case .callback: self = .callback
        }
    }
}

extension Chat.RoomToolbarAction {
    init(_ action: ChatToolbarAction) {
        self.init(
            actionId: action.actionId,
            label: action.label,
            icon: action.productIcon,
            handler: Chat.ProductActionHandler(action.handler)
        )
    }
}

extension ChatToolbarAction {
    /// The core refuses a sealed icon, so an icon is inline bytes or a preimage key.
    var productIcon: Chat.ProductResource {
        switch icon {
        case let .inline(mimeType, bytes): .inline(mimeType: mimeType, data: bytes)
        case let .preimage(key): .preimage(key: key)
        case .sealed: .inline(mimeType: "application/octet-stream", data: Data())
        }
    }
}

extension Chat.RoomSuggestion {
    init(_ suggestion: ChatSuggestion) {
        self.init(suggestionId: suggestion.suggestionId, text: suggestion.text, sendsText: suggestion.sendsText)
    }
}

extension Chat.RoomCommand {
    init(_ command: ChatCommandSpec) {
        self.init(name: command.name, description: command.description, argumentHint: command.argumentHint)
    }
}

extension Chat.RoomActivity {
    init(_ activity: ChatActivity) {
        self.init(text: activity.text, participantId: activity.participantId)
    }
}

extension Chat.RoomInputConfig {
    init(_ input: ChatInputConfig) {
        let payment: Chat.RoomInputPayment? =
            switch input.payment {
            case let .enabled(destination): .enabled(destination: destination)
            case nil: nil
            }

        self.init(
            placeholder: input.placeholder,
            photo: input.photo,
            video: input.video,
            file: input.file,
            payment: payment
        )
    }
}

extension Chat.RoomFooterConfig {
    init(_ footer: ChatRoomFooter) {
        switch footer {
        case let .input(input): self = .input(Chat.RoomInputConfig(input))
        case .empty: self = .empty
        case .custom: self = .custom
        }
    }
}
