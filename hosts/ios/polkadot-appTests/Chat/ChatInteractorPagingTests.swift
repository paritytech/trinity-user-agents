import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The room screen asks for pages only when the end of the window has more, and jumps to quoted messages (P5.3).
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
        #expect(model.holdsMessagesOutsideWindow)
    }

    @Test func anEndWithMoreMessagesAsksForAPage() async {
        engine.snapshotUpdates = [snapshot(older: .more, newer: .failed)]
        let interactor = makeInteractor()
        interactor.setup()
        await waitForListModel()

        interactor.loadOlderMessages()
        #expect(await nextCall() == .loadOlder)

        interactor.loadNewerMessages()
        #expect(await nextCall() == .loadNewer)
    }

    @Test func anEndWithoutMoreMessagesAsksNothing() async {
        engine.snapshotUpdates = [snapshot(older: .end, newer: .loading)]
        let interactor = makeInteractor()
        interactor.setup()
        await waitForListModel()

        interactor.loadOlderMessages()
        interactor.loadNewerMessages()
        interactor.jump(to: "m1")

        #expect(await nextCall() == .jump("m1"))
    }

    @Test func aJumpTellsThePresenterWhetherTheMessageExists() async {
        engine.jumpFindsMessage = false
        let interactor = makeInteractor()

        interactor.jump(to: "gone")

        #expect(await nextEvent { if case .jumped = $0 { true } else { false } } == .jumped("gone", found: false))
    }

    @Test func theEditsOfAProductMarkItsMessagesAsEdited() async throws {
        let message = Chat.LocalMessage(
            messageId: "m1",
            chatId: chatId,
            origin: .chatExtension("product.dot"),
            creationSource: .localDevice,
            status: .incoming(.seen),
            timestamp: 1_000,
            content: .text("now"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let model = await ChatInteractorState().onSnapshot(MessageListSnapshot(
            messages: [message],
            editedAt: ["m1": 2_000]
        ))

        #expect(model.isEdited("m1"))
        #expect(model.getLatestEdit(for: "m1")?.newText == "now")
        #expect(model.paging == .complete)
        #expect(!model.holdsMessagesOutsideWindow)
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

    func nextCall() async -> RecordingChatEngine.Call? {
        for await call in engine.calls {
            return call
        }
        return nil
    }
}
