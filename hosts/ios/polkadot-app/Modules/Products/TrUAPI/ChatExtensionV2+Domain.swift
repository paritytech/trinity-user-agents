import Foundation
import TrUAPIHost

// Maps the Chat Extension v2 values of the bindings to the Swift domain values of the chat store, and back
// where the core reads a value. The chat store and the notification extension do not link the bindings.

extension Chat.RoomConfig {
    init(_ config: ChatRoomConfig) {
        self.init(
            productSource: config.productSource,
            reactionDetails: config.reactionDetails,
            reportSeen: config.reportSeen
        )
    }

    var wireValue: ChatRoomConfig {
        ChatRoomConfig(productSource: productSource, reactionDetails: reactionDetails, reportSeen: reportSeen)
    }
}

extension Chat.RoomPreview {
    /// The preview with its last message as the native chat draws it. The status of a message of the contact says
    /// whether the user saw it.
    init(_ preview: ChatRoomPreview) {
        let badge = preview.badge.map(Chat.RoomBadge.init) ?? .hostDefault

        self.init(
            timestamp: preview.timestamp.map { Int64(clamping: $0) },
            badge: badge,
            lastMessage: preview.lastMessage.map { $0.previewMessage() },
            // The native chat list draws the heart for a reaction that the user did not see, and does not count it
            // in the unread badge.
            unseenReaction: preview.unseenReactions > 0
        )
    }
}

extension Chat.RoomBadge {
    init(_ badge: ChatRoomBadge) {
        switch badge {
        case .hidden: self = .none
        case let .unreadCount(count): self = .unreadCount(Int32(clamping: count))
        }
    }
}

extension Chat.RoomContact {
    init(_ contact: ChatRoomContact) {
        self.init(account: contact.account, username: contact.username, state: State(contact.state))
    }
}

extension Chat.RoomContact.State {
    init(_ state: ChatContactState) {
        switch state {
        case .invite: self = .invite
        case .outgoing: self = .outgoing
        case .incoming: self = .incoming
        case .established: self = .established
        case .blocked: self = .blocked
        case .peerLeft: self = .peerLeft
        }
    }
}

extension Chat.RoomFeatures {
    init(_ features: ChatRoomFeatures) {
        self.init(
            calls: features.calls.map { Chat.RoomCalls(audio: $0.audio, video: $0.video) },
            hopAttachments: features.hopAttachments
        )
    }

    var wireValue: ChatRoomFeatures {
        ChatRoomFeatures(
            calls: calls.map { ChatRoomCalls(audio: $0.audio, video: $0.video) },
            hopAttachments: hopAttachments
        )
    }
}

extension Chat.RoomCall {
    init(_ call: ChatRoomCall) {
        switch call {
        case let .ringing(callId, video, caller):
            self = .ringing(callId: callId, video: video, caller: caller)
        case let .active(callId, video, startedAt):
            self = .active(callId: callId, video: video, startedAt: startedAt)
        }
    }
}

extension Chat.RoomEvent {
    var wireValue: ChatRoomEvent {
        switch self {
        case let .callRequested(video, callId):
            .callRequested(video: video, callId: callId)
        case let .callAnswered(callId, accept):
            .callAnswered(callId: callId, accept: accept)
        case let .callEnded(callId):
            .callEnded(callId: callId)
        case let .callSignal(callId, signal):
            .callSignal(callId: callId, signal: signal)
        }
    }
}
