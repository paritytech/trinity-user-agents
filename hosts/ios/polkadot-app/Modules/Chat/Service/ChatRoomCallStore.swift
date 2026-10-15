import Foundation
import os

protocol ChatRoomCallStoring: Sendable {
    /// Shows or changes the call of a room, or removes it with nil.
    func set(_ call: Chat.RoomCall?, for chatId: Chat.Id)

    func call(for chatId: Chat.Id) -> Chat.RoomCall?

    /// Gives the call of the room now, and then each change of it.
    func callUpdates(for chatId: Chat.Id) -> AsyncStream<Chat.RoomCall?>
}

/// Keeps the call of each product room in memory (07-LATER §11.2). A call tells what happens now, so the host does
/// not keep it across a restart of the app. The room screen of P10.7 draws it.
final class ChatRoomCallStore: ChatRoomCallStoring, @unchecked Sendable {
    static let shared = ChatRoomCallStore()

    private struct State {
        var calls: [String: Chat.RoomCall] = [:]
        var observers: [String: [UUID: AsyncStream<Chat.RoomCall?>.Continuation]] = [:]
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    func set(_ call: Chat.RoomCall?, for chatId: Chat.Id) {
        let key = chatId.rawRepresentation
        state.withLockUnchecked { state in
            state.calls[key] = call
            for observer in (state.observers[key] ?? [:]).values {
                observer.yield(call)
            }
        }
    }

    func call(for chatId: Chat.Id) -> Chat.RoomCall? {
        state.withLockUnchecked { $0.calls[chatId.rawRepresentation] }
    }

    func callUpdates(for chatId: Chat.Id) -> AsyncStream<Chat.RoomCall?> {
        let key = chatId.rawRepresentation
        let token = UUID()
        let (stream, continuation) = AsyncStream<Chat.RoomCall?>.makeStream(bufferingPolicy: .bufferingNewest(1))

        // The first value goes out under the lock, so a concurrent change cannot reach the stream before it.
        state.withLockUnchecked { state in
            state.observers[key, default: [:]][token] = continuation
            continuation.yield(state.calls[key])
        }

        continuation.onTermination = { [weak self] _ in
            self?.state.withLockUnchecked { state in
                state.observers[key]?[token] = nil
            }
        }

        return stream
    }
}
