import Foundation
import PolkadotUI
@testable import polkadot_app

/// Records what the chat interactor reports to its presenter. Each report is also yielded on `events`.
@MainActor
final class RecordingChatInteractorOutput: ChatInteractorOutputProtocol {
    enum Event: Equatable {
        case metadata(Chat.Id)
        case listModel
        case footer
        case leftChat
        case blockedUser
        case declinedChatRequest
        case removedChat
        case jumped(Chat.MessageId, found: Bool)
    }

    /// The last list model that the interactor gave.
    private(set) var listModel: MessageListModel?

    nonisolated let events: AsyncStream<Event>
    private let continuation: AsyncStream<Event>.Continuation

    nonisolated init() {
        (events, continuation) = AsyncStream.makeStream()
    }

    func didReceive(metadata: MessageListMetadata) {
        continuation.yield(.metadata(metadata.chatMetadata.chatId))
    }

    func didReceive(listModel: MessageListModel) {
        self.listModel = listModel
        continuation.yield(.listModel)
    }

    func didJump(to messageId: Chat.MessageId, found: Bool) {
        continuation.yield(.jumped(messageId, found: found))
    }

    func didReceiveFooter(_: (any HashableContentConfiguration)?) {
        continuation.yield(.footer)
    }

    func didLeftChat() {
        continuation.yield(.leftChat)
    }

    func didBlockUser() {
        continuation.yield(.blockedUser)
    }

    func didDeclineChatRequest() {
        continuation.yield(.declinedChatRequest)
    }

    func didRemoveChat() {
        continuation.yield(.removedChat)
    }
}
