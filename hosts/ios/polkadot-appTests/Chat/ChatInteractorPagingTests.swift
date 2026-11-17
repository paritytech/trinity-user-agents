import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The room screen reads the window of messages and its paging state from the message source.
@MainActor
struct ChatInteractorPagingTests {
    private let chatId = Chat.Id.chatExtension("product.dot", roomId: "room")
    private let engine = RecordingChatEngine()
    private let output = RecordingChatInteractorOutput()

    @Test func thePagingStateOfTheWindowReachesThePresenter() async throws {
        engine.snapshotUpdates = [snapshot(older: .loading, newer: .end, readiness: .loading)]
        let interactor = makeInteractor()

        interactor.setup()
        await waitForListModel()

        let model = try #require(output.listModel)
        #expect(model.paging == ChatPagingState(older: .loading, newer: .end))
        #expect(model.readiness == .loading)
    }

    /// The "New messages" marker waits for the first page of a room that pages.
    @Test func theFirstUnreadMessageWaitsForTheFirstPage() async {
        let state = ChatInteractorState()
        let unread = message("m1", status: .incoming(.new))

        let loading = await state.onSnapshot(MessageListSnapshot(messages: [], readiness: .loading))
        let ready = await state.onSnapshot(MessageListSnapshot(messages: [unread]))

        #expect(loading.initiallyUnreadMessage == nil)
        #expect(ready.initiallyUnreadMessage == "m1")
        #expect(ready.paging == .complete)
    }
}

private extension ChatInteractorPagingTests {
    func makeInteractor() -> ChatInteractor {
        let reactions = ChatReactionRepository(
            repository: AnyDataProviderRepository(InMemoryDataProviderRepository<Chat.MessageReaction>())
        )
        let interactor = ChatInteractor(
            chatId: chatId,
            engine: engine,
            reactionRepository: reactions,
            usernameStorage: StubUsernameStorage(),
            foregroundVisibilityReporter: nil,
            notificationsCleaner: StubPushNotificationsCleaner()
        )
        interactor.presenter = output
        return interactor
    }

    func snapshot(
        older: ChatPagingState.Edge,
        newer: ChatPagingState.Edge,
        readiness: ChatRoomReadiness = .ready
    ) -> MessageListSnapshot {
        MessageListSnapshot(messages: [], paging: ChatPagingState(older: older, newer: newer), readiness: readiness)
    }

    func message(_ id: String, status: Chat.LocalMessage.Status) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: id,
            chatId: chatId,
            origin: .chatExtension("product.dot"),
            creationSource: .localDevice,
            status: status,
            timestamp: 1_000,
            content: .text(id),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    func waitForListModel() async {
        _ = await nextEvent { $0 == .listModel }
    }

    func nextEvent(
        where matches: (RecordingChatInteractorOutput.Event) -> Bool
    ) async -> RecordingChatInteractorOutput.Event? {
        for await event in output.events where matches(event) {
            return event
        }
        return nil
    }
}
