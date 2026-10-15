import Foundation
import os

protocol ChatPreparingMessageTracking: Sendable {
    /// Records that a message of a participant of the product has the status `Preparing`, or that it left it.
    func setPreparing(_ preparing: Bool, productMessageId: String, chatId: Chat.Id)

    /// The ids under which the product knows the messages of a room that have the status `Preparing`.
    func preparingMessageIds(in chatId: Chat.Id) -> Set<String>

    /// Gives the ids now, and then after each change.
    func preparingUpdates(for chatId: Chat.Id) -> AsyncStream<Set<String>>
}

/// Keeps in memory which messages of the product have the status `Preparing` (07-LATER §9.4).
///
/// The chat store keeps the read state of a message of a participant of the product, not the status that the
/// product gives it. A reasoning row reads "Thinking" while its message is `Preparing`, so the host records the
/// status from the feed updates. After a restart of the app, no message is `Preparing`.
final class ChatPreparingMessageStore: ChatPreparingMessageTracking, @unchecked Sendable {
    static let shared = ChatPreparingMessageStore()

    private struct State {
        var preparing: [String: Set<String>] = [:]
        var observers: [String: [UUID: AsyncStream<Set<String>>.Continuation]] = [:]
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    func setPreparing(_ preparing: Bool, productMessageId: String, chatId: Chat.Id) {
        let key = chatId.rawRepresentation
        state.withLockUnchecked { state in
            var ids = state.preparing[key] ?? []
            let changed = preparing
                ? ids.insert(productMessageId).inserted
                : ids.remove(productMessageId) != nil
            guard changed else { return }

            state.preparing[key] = ids.isEmpty ? nil : ids
            for observer in (state.observers[key] ?? [:]).values {
                observer.yield(ids)
            }
        }
    }

    func preparingMessageIds(in chatId: Chat.Id) -> Set<String> {
        state.withLockUnchecked { $0.preparing[chatId.rawRepresentation] ?? [] }
    }

    func preparingUpdates(for chatId: Chat.Id) -> AsyncStream<Set<String>> {
        let key = chatId.rawRepresentation
        let token = UUID()
        let (stream, continuation) = AsyncStream<Set<String>>.makeStream(bufferingPolicy: .bufferingNewest(1))

        state.withLockUnchecked { state in
            state.observers[key, default: [:]][token] = continuation
            continuation.yield(state.preparing[key] ?? [])
        }

        continuation.onTermination = { [weak self] _ in
            self?.state.withLockUnchecked { state in
                state.observers[key]?[token] = nil
            }
        }

        return stream
    }
}

extension ChatPreparingMessageTracking {
    /// Records the status that a feed update of the product gives a message.
    func record(_ update: Chat.ProductFeedUpdate, isPreparing: Bool, chatId: Chat.Id) {
        switch update {
        case let .new(message):
            setPreparing(isPreparing, productMessageId: message.messageId, chatId: chatId)
        case let .statusChanged(messageId, status):
            setPreparing(status == .preparing, productMessageId: messageId, chatId: chatId)
        case let .deleted(messageId):
            setPreparing(false, productMessageId: messageId, chatId: chatId)
        case .edited,
             .reactionPlaced,
             .reactionRemoved:
            break
        }
    }
}
