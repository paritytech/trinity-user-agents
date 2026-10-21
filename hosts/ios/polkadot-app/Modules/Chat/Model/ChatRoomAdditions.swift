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
        let preimageAttachments: Bool
        /// The product answers `search_query`. Only a room that the product keeps reads it.
        let search: Bool
        /// Nil shows no call button.
        let calls: RoomCalls?
        let readAloud: Bool
        /// A user attachment above the inline bound is sealed: the core encrypts it and stores the ciphertext.
        let sealedAttachments: Bool
        /// A user photo, video or file of the chat product goes to a HOP node, as the native chat sends one.
        let hopAttachments: Bool

        init(
            preimageAttachments: Bool,
            search: Bool,
            calls: RoomCalls?,
            readAloud: Bool,
            sealedAttachments: Bool = false,
            hopAttachments: Bool = false
        ) {
            self.preimageAttachments = preimageAttachments
            self.search = search
            self.calls = calls
            self.readAloud = readAloud
            self.sealedAttachments = sealedAttachments
            self.hopAttachments = hopAttachments
        }

        /// The features of a room that declares none.
        static let hostDefault = RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            readAloud: false
        )
    }

    /// What the user did with a product room: mute and archive (`ChatRoomUserState`). The user sets it, not
    /// the product. The host keeps it beside the declarations of the room.
    struct RoomUserState: Equatable {
        /// Milliseconds since the Unix epoch when the mute ends. ``RoomUserState/mutedForever`` mutes the room
        /// with no end. Nil: the room is not muted.
        let mutedUntil: UInt64?
        let archived: Bool

        /// The value of `mutedUntil` that mutes a room with no end (`u64::MAX`).
        static let mutedForever = UInt64.max

        /// The state of a room that the user did not mute or archive.
        static let `default` = RoomUserState(mutedUntil: nil, archived: false)

        var isDefault: Bool {
            self == .default
        }

        /// True when the room is muted at `date`.
        func isMuted(at date: Date) -> Bool {
            guard let mutedUntil else { return false }
            return mutedUntil == Self.mutedForever || mutedUntil > date.toChatTimestamp()
        }
    }

    /// The user state of one room of a product, as the core reads it.
    struct RoomUserStateEntry: Equatable {
        let roomId: String
        let state: RoomUserState
    }

    /// A name and an icon that a product speaks as in its own rooms (07-LATER §5). The participant id is
    /// `bot:<botId>`.
    struct ProductPersona: Equatable {
        let productId: String
        let botId: String
        let name: String
        /// The source of the icon: an `https` address or an inline `data:` address. Nil draws the initial of the
        /// name.
        let icon: String?
    }

    /// The call of a room (`ChatRoomCall`). The host keeps it in memory: a call does not survive a restart.
    enum RoomCall: Equatable {
        /// A call to the user. `caller` is a participant id; nil is the product.
        case ringing(callId: String, video: Bool, caller: String?)
        /// A call that continues. `startedAt` is in milliseconds since the Unix epoch. `mediaSession` is the
        /// 32-byte id of a session of the Media service.
        case active(callId: String, video: Bool, startedAt: UInt64, mediaSession: Data?)
    }

    /// What the user does with the additions in a room, for the product (`ChatRoomEvent`).
    enum RoomEvent: Equatable {
        case userStateChanged(RoomUserState)
        case woken(watchId: String)
        case callRequested(video: Bool)
        case callAnswered(callId: String, accept: Bool)
        case callOpened(callId: String)
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

    /// Replaces the mute and the archive state of a room.
    struct RoomUserStateUpdate {
        let chatId: Chat.Id
        let state: RoomUserState
    }
}
