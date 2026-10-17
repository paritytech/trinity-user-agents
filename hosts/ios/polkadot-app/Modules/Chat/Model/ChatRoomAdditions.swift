import Foundation

// The Swift domain values of the Chat Extension v2 additions (07-LATER §3.2). The chat store and the notification
// extension use these values, because they do not link the bindings.

extension Chat {
    /// The call buttons in the header of a room (`ChatRoomCalls`).
    struct RoomCalls: Equatable {
        let audio: Bool
        let video: Bool
    }

    /// The message of another room of the product that a thread room answers (`ChatThreadParent`).
    struct ThreadParent: Equatable {
        let roomId: String
        let messageId: String
    }

    /// The optional features that a product declares for a room (`ChatRoomFeatures`). The host keeps a feature
    /// that it does not serve, so that a later build applies it.
    struct RoomFeatures: Equatable {
        let preimageAttachments: Bool
        /// The product answers `search_query`. Only a room that the product keeps reads it.
        let search: Bool
        /// Nil shows no call button.
        let calls: RoomCalls?
        /// The room is the thread of a message of another room. The chat list does not show a thread room.
        let threadOf: ThreadParent?
        let readAloud: Bool
        /// A user attachment above the inline bound is sealed: the core encrypts it and stores the ciphertext.
        let sealedAttachments: Bool

        init(
            preimageAttachments: Bool,
            search: Bool,
            calls: RoomCalls?,
            threadOf: ThreadParent?,
            readAloud: Bool,
            sealedAttachments: Bool = false
        ) {
            self.preimageAttachments = preimageAttachments
            self.search = search
            self.calls = calls
            self.threadOf = threadOf
            self.readAloud = readAloud
            self.sealedAttachments = sealedAttachments
        }

        /// The features of a room that declares none.
        static let hostDefault = RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            threadOf: nil,
            readAloud: false
        )
    }

    /// The summary under a message that has a thread (`ChatThreadSummary`).
    struct ThreadSummary: Equatable {
        /// The thread room. Its `threadOf` names the message.
        let roomId: String
        let replyCount: UInt32
        /// Milliseconds since the Unix epoch.
        let lastReplyAt: UInt64?
        /// Up to three participants that replied.
        let participantIds: [String]
    }

    /// Which alternative of a message the room shows (`ChatBranchPosition`). The index starts at 1.
    struct BranchPosition: Equatable {
        let index: UInt16
        let count: UInt16
    }

    /// The message is the reasoning of the product (`ChatReasoning`).
    struct Reasoning: Equatable {
        /// How long the product thought, in milliseconds.
        let durationMs: UInt32?
    }

    /// The thread summary, the branch position and the reasoning flag of one message (`ChatMessageMeta`).
    struct MessageMeta: Equatable {
        let thread: ThreadSummary?
        let branch: BranchPosition?
        let reasoning: Reasoning?

        /// The meta of a message that has no thread, no branch and no reasoning flag.
        static let none = MessageMeta(thread: nil, branch: nil, reasoning: nil)

        var isEmpty: Bool {
            self == .none
        }
    }

    /// The meta of the messages of one room, by the id under which the product knows each message.
    struct MessageMetaIndex: Equatable {
        private let metaByMessageId: [String: MessageMeta]

        init(_ metaByMessageId: [String: MessageMeta] = [:]) {
            self.metaByMessageId = metaByMessageId
        }

        static let empty = MessageMetaIndex()

        var count: Int {
            metaByMessageId.count
        }

        var isEmpty: Bool {
            metaByMessageId.isEmpty
        }

        /// The meta of a message, or nil when the product set none.
        func meta(for productMessageId: String) -> MessageMeta? {
            metaByMessageId[productMessageId]
        }

        /// The ids of the messages that have meta, in alphabetical order.
        var productMessageIds: [String] {
            metaByMessageId.keys.sorted()
        }
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
        case branchSelected(messageId: String, index: UInt16)
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
