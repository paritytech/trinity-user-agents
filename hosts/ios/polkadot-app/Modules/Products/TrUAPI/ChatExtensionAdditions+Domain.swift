import Foundation
import TrUAPIHost

// Maps the values of the Chat Extension v2 additions in the bindings to the Swift domain values of the chat store,
// and back where the core reads a value.

extension Chat.RoomFeatures {
    init(_ features: ChatRoomFeatures) {
        self.init(
            preimageAttachments: features.preimageAttachments,
            search: features.search,
            calls: features.calls.map { Chat.RoomCalls(audio: $0.audio, video: $0.video) },
            readAloud: features.readAloud,
            sealedAttachments: features.sealedAttachments,
            hopAttachments: features.hopAttachments
        )
    }

    var wireValue: ChatRoomFeatures {
        ChatRoomFeatures(
            preimageAttachments: preimageAttachments,
            search: search,
            calls: calls.map { ChatRoomCalls(audio: $0.audio, video: $0.video) },
            readAloud: readAloud,
            sealedAttachments: sealedAttachments,
            hopAttachments: hopAttachments
        )
    }
}

extension Chat.RoomCall {
    init(_ call: ChatRoomCall) {
        switch call {
        case let .ringing(callId, video, caller):
            self = .ringing(callId: callId, video: video, caller: caller)
        case let .active(callId, video, startedAt, mediaSession):
            self = .active(callId: callId, video: video, startedAt: startedAt, mediaSession: mediaSession)
        }
    }
}

extension Chat.RoomUserStateEntry {
    var wireValue: ChatRoomUserState {
        ChatRoomUserState(roomId: roomId, mutedUntil: state.mutedUntil, archived: state.archived)
    }
}

extension Chat.RoomEvent {
    var wireValue: ChatRoomEvent {
        switch self {
        case let .userStateChanged(state):
            .userStateChanged(mutedUntil: state.mutedUntil, archived: state.archived)
        case let .woken(watchId):
            .woken(watchId: watchId)
        case let .callRequested(video):
            .callRequested(video: video)
        case let .callAnswered(callId, accept):
            .callAnswered(callId: callId, accept: accept)
        case let .callOpened(callId):
            .callOpened(callId: callId)
        case let .callEnded(callId):
            .callEnded(callId: callId)
        case let .callSignal(callId, signal):
            .callSignal(callId: callId, signal: signal)
        }
    }
}
