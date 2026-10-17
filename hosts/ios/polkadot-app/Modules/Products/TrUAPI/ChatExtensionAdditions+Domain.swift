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
            threadOf: features.threadOf.map { Chat.ThreadParent(roomId: $0.roomId, messageId: $0.messageId) },
            readAloud: features.readAloud,
            sealedAttachments: features.sealedAttachments,
            inputEvents: features.inputEvents
        )
    }

    var wireValue: ChatRoomFeatures {
        ChatRoomFeatures(
            preimageAttachments: preimageAttachments,
            search: search,
            calls: calls.map { ChatRoomCalls(audio: $0.audio, video: $0.video) },
            threadOf: threadOf.map { ChatThreadParent(roomId: $0.roomId, messageId: $0.messageId) },
            readAloud: readAloud,
            sealedAttachments: sealedAttachments,
            inputEvents: inputEvents
        )
    }
}

extension Chat.MessageMeta {
    init(_ meta: ChatMessageMeta) {
        self.init(
            thread: meta.thread.map {
                Chat.ThreadSummary(
                    roomId: $0.roomId,
                    replyCount: $0.replyCount,
                    lastReplyAt: $0.lastReplyAt,
                    participantIds: $0.participantIds
                )
            },
            branch: meta.branch.map { Chat.BranchPosition(index: $0.index, count: $0.count) },
            reasoning: meta.reasoning.map { Chat.Reasoning(durationMs: $0.durationMs) }
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
        case let .branchSelected(messageId, index):
            .branchSelected(messageId: messageId, index: index)
        }
    }
}
