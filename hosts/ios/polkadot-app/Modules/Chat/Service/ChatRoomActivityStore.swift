import Foundation
import os

protocol ChatRoomActivityStoring: Sendable {
    /// Shows the activity line of a room, or removes it with nil.
    func set(_ activity: Chat.RoomActivity?, for chatId: Chat.Id)

    func activity(for chatId: Chat.Id) -> Chat.RoomActivity?

    /// Gives the line of the room now, and then each change of it.
    func activityUpdates(for chatId: Chat.Id) -> AsyncStream<Chat.RoomActivity?>
}

/// Keeps the activity line of each product room in memory, for example "Bot is typing" (02-DESIGN §6.6).
///
/// The line tells what happens now. The product removes it, and the room screen removes it when the room
/// closes (P6.8). So the host does not keep it across a restart of the app.
final class ChatRoomActivityStore: ChatRoomActivityStoring, @unchecked Sendable {
    static let shared = ChatRoomActivityStore()

    private struct State {
        var activities: [String: Chat.RoomActivity] = [:]
        var observers: [String: [UUID: AsyncStream<Chat.RoomActivity?>.Continuation]] = [:]
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    func set(_ activity: Chat.RoomActivity?, for chatId: Chat.Id) {
        let key = chatId.rawRepresentation
        state.withLockUnchecked { state in
            state.activities[key] = activity
            for observer in (state.observers[key] ?? [:]).values {
                observer.yield(activity)
            }
        }
    }

    func activity(for chatId: Chat.Id) -> Chat.RoomActivity? {
        state.withLockUnchecked { $0.activities[chatId.rawRepresentation] }
    }

    func activityUpdates(for chatId: Chat.Id) -> AsyncStream<Chat.RoomActivity?> {
        let key = chatId.rawRepresentation
        let token = UUID()
        let (stream, continuation) = AsyncStream<Chat.RoomActivity?>.makeStream(bufferingPolicy: .bufferingNewest(1))

        // The first value goes out under the lock, so a concurrent change cannot reach the stream before it.
        state.withLockUnchecked { state in
            state.observers[key, default: [:]][token] = continuation
            continuation.yield(state.activities[key])
        }

        continuation.onTermination = { [weak self] _ in
            self?.state.withLockUnchecked { state in
                state.observers[key]?[token] = nil
            }
        }

        return stream
    }
}
