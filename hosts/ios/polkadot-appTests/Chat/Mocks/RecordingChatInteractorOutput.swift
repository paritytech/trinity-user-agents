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
    }

    nonisolated let events: AsyncStream<Event>
    private let continuation: AsyncStream<Event>.Continuation

    nonisolated init() {
        (events, continuation) = AsyncStream.makeStream()
    }

    func didReceive(metadata: MessageListMetadata) {
        continuation.yield(.metadata(metadata.chatMetadata.chatId))
    }

    func didReceive(listModel _: MessageListModel) {
        continuation.yield(.listModel)
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
