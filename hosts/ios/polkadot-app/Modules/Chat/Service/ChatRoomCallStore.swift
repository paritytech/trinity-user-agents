import Foundation
import os

protocol ChatRoomCallStoring: Sendable {
    /// Shows or changes the call of a room, or removes it with nil.
    func set(_ call: Chat.RoomCall?, for chatId: Chat.Id)

    func call(for chatId: Chat.Id) -> Chat.RoomCall?

    /// Gives the call of the room now, and then each change of it.
    func callUpdates(for chatId: Chat.Id) -> AsyncStream<Chat.RoomCall?>

    /// Gives each change of the call of any room, from now.
    func changes() -> AsyncStream<Chat.RoomCallChange>

    /// The rooms that have a call now, with their calls.
    func allCalls() -> [Chat.RoomCallChange]

    /// Removes each ringing call of the rooms of a product. The host does this when the worker of the product stops
    /// (07-LATER §11.2).
    func clearRinging(ofProduct productId: ChatExtension.Id)
}

extension Chat {
    /// The call of one room after a change. A nil call: the room has no call now.
    struct RoomCallChange: Equatable, Sendable {
        let chatId: Chat.Id
        let call: RoomCall?
    }
}

/// Keeps the call of each product room in memory (07-LATER §11.2). A call tells what happens now, so the host does
/// not keep it across a restart of the app.
///
/// A ringing call stays until the product clears it or its worker stops: it rings until the caller ends it or the
/// user answers, as the native chat rings. The core holds the worker of the product for as long.
final class ChatRoomCallStore: ChatRoomCallStoring, @unchecked Sendable {
    static let shared = ChatRoomCallStore()

    private struct State {
        var calls: [String: Chat.RoomCallChange] = [:]
        var observers: [String: [UUID: AsyncStream<Chat.RoomCall?>.Continuation]] = [:]
        var changeObservers: [UUID: AsyncStream<Chat.RoomCallChange>.Continuation] = [:]
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    func set(_ call: Chat.RoomCall?, for chatId: Chat.Id) {
        let key = chatId.rawRepresentation
        let change = Chat.RoomCallChange(chatId: chatId, call: call)
        state.withLockUnchecked { state in
            state.calls[key] = call == nil ? nil : change
            for observer in (state.observers[key] ?? [:]).values {
                observer.yield(call)
            }
            for observer in state.changeObservers.values {
                observer.yield(change)
            }
        }
    }

    func call(for chatId: Chat.Id) -> Chat.RoomCall? {
        state.withLockUnchecked { $0.calls[chatId.rawRepresentation]?.call }
    }

    func callUpdates(for chatId: Chat.Id) -> AsyncStream<Chat.RoomCall?> {
        let key = chatId.rawRepresentation
        let token = UUID()
        let (stream, continuation) = AsyncStream<Chat.RoomCall?>.makeStream(bufferingPolicy: .bufferingNewest(1))

        // The first value goes out under the lock, so a concurrent change cannot reach the stream before it.
        state.withLockUnchecked { state in
            state.observers[key, default: [:]][token] = continuation
            continuation.yield(state.calls[key]?.call)
        }

        continuation.onTermination = { [weak self] _ in
            self?.state.withLockUnchecked { state in
                state.observers[key]?[token] = nil
            }
        }

        return stream
    }

    func changes() -> AsyncStream<Chat.RoomCallChange> {
        let token = UUID()
        let (stream, continuation) = AsyncStream<Chat.RoomCallChange>.makeStream()
        state.withLockUnchecked { $0.changeObservers[token] = continuation }

        continuation.onTermination = { [weak self] _ in
            self?.state.withLockUnchecked { $0.changeObservers[token] = nil }
        }

        return stream
    }

    func allCalls() -> [Chat.RoomCallChange] {
        state.withLockUnchecked { state in
            state.calls.keys.sorted().compactMap { state.calls[$0] }
        }
    }

    func clearRinging(ofProduct productId: ChatExtension.Id) {
        for change in allCalls() where change.chatId.extensionId == productId {
            if case .ringing = change.call {
                clear(change.call, of: change.chatId)
            }
        }
    }
}

private extension ChatRoomCallStore {
    /// Removes the call of the room when it is still `call`.
    func clear(_ call: Chat.RoomCall?, of chatId: Chat.Id) {
        let key = chatId.rawRepresentation
        state.withLockUnchecked { state in
            guard state.calls[key]?.call == call else { return }
            state.calls[key] = nil
            let change = Chat.RoomCallChange(chatId: chatId, call: nil)
            for observer in (state.observers[key] ?? [:]).values {
                observer.yield(nil)
            }
            for observer in state.changeObservers.values {
                observer.yield(change)
            }
        }
    }
}
