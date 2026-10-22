import Foundation

// The Swift domain values of the Chat Extension v2 additions (07-LATER §3.2). The chat store and the notification
// extension use these values, because they do not link the bindings.

extension Chat {
    /// The call buttons in the header of a room (`ChatRoomCalls`).
    struct RoomCalls: Equatable {
        let audio: Bool
        let video: Bool
    }

    /// The optional features that a product declares for a room (`ChatRoomFeatures`). The host keeps a feature
    /// that it does not serve, so that a later build applies it.
    struct RoomFeatures: Equatable {
        /// Nil shows no call button.
        let calls: RoomCalls?
        /// A user photo or video of the chat product goes to a HOP node, as the native chat sends one.
        let hopAttachments: Bool

        /// The features of a room that declares none.
        static let hostDefault = RoomFeatures(calls: nil, hopAttachments: false)
    }

    /// The call of a room (`ChatRoomCall`). The host keeps it in memory: a call does not survive a restart.
    enum RoomCall: Equatable {
        /// A call to the user. `caller` is a participant id; nil is the product.
        case ringing(callId: String, video: Bool, caller: String?)
        /// A call that continues. `startedAt` is in milliseconds since the Unix epoch.
        case active(callId: String, video: Bool, startedAt: UInt64)
    }

    /// What the user does with the additions in a room, for the product (`ChatRoomEvent`).
    enum RoomEvent: Equatable {
        /// The user starts a call. `callId` is the id that the host chose: the product uses it for the call and for its
        /// offer message.
        case callRequested(video: Bool, callId: String)
        case callAnswered(callId: String, accept: Bool)
        case callEnded(callId: String)
        /// A media signal of the call of this host for the other side (`callMedia`).
        case callSignal(callId: String, signal: Data)
    }
}

extension Chat {
    /// Replaces the features of a room whole.
    struct RoomFeaturesUpdate {
        let chatId: Chat.Id
        let features: RoomFeatures
    }
}
