import AsyncExtensions
import Foundation
import PolkadotUI
@testable import polkadot_app

/// Records what a room screen asks of its engine. Each call is also yielded on `calls`, so a test can await
/// work that the interactor starts in a task of its own.
final class RecordingChatEngine: ChatEngineProtocol, @unchecked Sendable {
    enum Call: Equatable {
        case sendUserMessage(Chat.LocalMessage.Content)
        case markAsSeen(Set<Chat.MessageId>)
        case loadOlder
        case loadNewer
        case jump(Chat.MessageId)
        case perform(ChatSourceAction)
    }

    /// What `chatMetadataStream` yields before it finishes.
    var metadataUpdates: [ChatMetadata?] = []
    /// What `messageSnapshots` yields. The stream stays open, as the stream of a room screen does.
    var snapshotUpdates: [MessageListSnapshot] = []
    var jumpFindsMessage = true
    var actionOutcome = ChatSourceActionOutcome.done
    /// What `sendUserMessage` throws after it records the call.
    var sendError: Error?

    let calls: AsyncStream<Call>
    private let continuation: AsyncStream<Call>.Continuation

    init() {
        (calls, continuation) = AsyncStream.makeStream()
    }

    func sendUserMessage(with content: Chat.LocalMessage.Content) async throws {
        continuation.yield(.sendUserMessage(content))

        if let sendError {
            throw sendError
        }
    }

    func markAsSeen(messageIds: Set<Chat.MessageId>) async throws {
        continuation.yield(.markAsSeen(messageIds))
    }

    func leaveChat() async throws {}
    func blockUser() async throws {}
    func unblockUser() async throws {}
    func acceptChatRequest() async throws {}
    func declineChatRequest() async throws {}

    func chatMetadataStream() -> AnyAsyncSequence<ChatMetadata?> {
        let updates = metadataUpdates
        return AsyncStream<ChatMetadata?> { continuation in
            updates.forEach { continuation.yield($0) }
            continuation.finish()
        }
        .eraseToAnyAsyncSequence()
    }

    func footerStream() async throws -> AnyAsyncSequence<(any HashableContentConfiguration)?>? {
        nil
    }

    func processAction(_: Chat.Action) async {}

    func messageSnapshots() async throws -> AnyAsyncSequence<MessageListSnapshot> {
        guard !snapshotUpdates.isEmpty else {
            return AsyncStream<MessageListSnapshot> { $0.finish() }.eraseToAnyAsyncSequence()
        }

        let updates = snapshotUpdates
        return AsyncStream<MessageListSnapshot> { continuation in
            updates.forEach { continuation.yield($0) }
        }
        .eraseToAnyAsyncSequence()
    }

    func loadOlder() async -> ChatPageOutcome {
        continuation.yield(.loadOlder)
        return .appended(0)
    }

    func loadNewer() async -> ChatPageOutcome {
        continuation.yield(.loadNewer)
        return .appended(0)
    }

    func jump(to messageId: Chat.MessageId) async -> Bool {
        continuation.yield(.jump(messageId))
        return jumpFindsMessage
    }

    func perform(_ action: ChatSourceAction) async throws -> ChatSourceActionOutcome {
        continuation.yield(.perform(action))
        return actionOutcome
    }
}
