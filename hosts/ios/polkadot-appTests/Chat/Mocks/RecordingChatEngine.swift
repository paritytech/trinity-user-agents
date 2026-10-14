import AsyncExtensions
import Foundation
import PolkadotUI
@testable import polkadot_app

/// Records what a room screen asks of its engine. Each call is also yielded on `calls`, so a test can await
/// work that the interactor starts in a task of its own.
final class RecordingChatEngine: ChatEngineProtocol, @unchecked Sendable {
    enum Call: Equatable {
        case sendUserMessage(Chat.LocalMessage.Content)
        case sendCommand(Chat.Command)
    }

    var takesCommands = true
    /// What `chatMetadataStream` yields before it finishes.
    var metadataUpdates: [ChatMetadata?] = []

    let calls: AsyncStream<Call>
    private let continuation: AsyncStream<Call>.Continuation

    init() {
        (calls, continuation) = AsyncStream.makeStream()
    }

    func sendUserMessage(with content: Chat.LocalMessage.Content) async throws {
        continuation.yield(.sendUserMessage(content))
    }

    func sendCommand(_ command: Chat.Command) async -> Bool {
        continuation.yield(.sendCommand(command))
        return takesCommands
    }

    func subscribe() async throws -> AnyAsyncSequence<[Chat.LocalMessage]> {
        AsyncStream<[Chat.LocalMessage]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func markAsSeen(messageIds _: Set<Chat.MessageId>) async throws {}
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
}
