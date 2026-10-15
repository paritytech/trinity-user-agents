import Foundation

/// What the room screen shows for the call of a product room (07-LATER §11.2): a ringing banner, or a call bar.
struct ChatRoomCallViewModel: Equatable {
    enum State: Equatable {
        /// A call to the user. Accept and Decline answer it.
        case ringing(caller: Caller)
        /// A call that continues. A tap opens it, and End ends it. `hasMedia`: a session of the Media service carries
        /// the call, so the host opens its own call screen.
        case active(startedAt: Date, hasMedia: Bool)
    }

    /// The participant that calls, as the room draws an author.
    struct Caller: Equatable {
        let name: String
        let initials: String
        let colorSeed: String
    }

    let callId: String
    let video: Bool
    let state: State

    var isRinging: Bool {
        if case .ringing = state { true } else { false }
    }

    /// The title of the ringing banner and of the list preview: "Incoming call" or "Incoming video call".
    static func ringingTitle(video: Bool) -> String {
        String(localized: video ? .ChatCall.ringingTitleVideo : .ChatCall.ringingTitle)
    }

    /// The title of the call bar: "Call" or "Video call".
    static func barTitle(video: Bool) -> String {
        String(localized: video ? .ChatCall.barTitleVideo : .ChatCall.barTitle)
    }

    /// The time since the start of the call: `m:ss`, or `h:mm:ss` from one hour.
    static func elapsedText(since start: Date, now: Date = Date()) -> String {
        let seconds = max(Int(now.timeIntervalSince(start)), 0)
        let hours = seconds / 3_600
        let minutes = seconds % 3_600 / 60
        let rest = seconds % 60
        return hours > 0
            ? String(format: "%d:%02d:%02d", hours, minutes, rest)
            : String(format: "%d:%02d", minutes, rest)
    }
}

extension Chat.RoomCall {
    var callId: String {
        switch self {
        case let .ringing(callId, _, _),
             let .active(callId, _, _, _): callId
        }
    }

    var isVideo: Bool {
        switch self {
        case let .ringing(_, video, _),
             let .active(_, video, _, _): video
        }
    }

    /// The call as the E2E harness reads it: `ringing <id> video=<bool> caller=<id>`, `active <id> video=<bool>
    /// media=<bool>`.
    var e2eDescription: String {
        switch self {
        case let .ringing(callId, video, caller):
            "ringing \(callId) video=\(video) caller=\(caller ?? "product")"
        case let .active(callId, video, _, mediaSession):
            "active \(callId) video=\(video) media=\(mediaSession != nil)"
        }
    }
}

extension ChatRoomPolicy {
    /// The call buttons of the header as the E2E harness reads them: `audio,video`, or `none`.
    var callButtonsE2EDescription: String {
        let names = toolbar.compactMap { action -> String? in
            switch action {
            case .audioCall: "audio"
            case .videoCall: "video"
            default: nil
            }
        }
        return names.isEmpty ? "none" : names.joined(separator: ",")
    }
}

extension ChatRoomCallViewModel {
    /// The preview of the chat-list row of a room that rings: "Incoming call" as an activity that continues. Nil
    /// when the room does not ring.
    static func listPreview(of call: Chat.RoomCall?) -> String? {
        guard case let .ringing(_, video, _) = call else { return nil }
        return ringingTitle(video: video)
    }
}
