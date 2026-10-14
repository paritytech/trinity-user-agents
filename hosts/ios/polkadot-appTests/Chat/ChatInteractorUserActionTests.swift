import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// In a product-source room the actions of the user go to the message source, and so to `user_action`
/// (P5.5, P5.6, 02-DESIGN §13 item 11).
@MainActor
struct ChatInteractorUserActionTests {
    private let chatId = Chat.Id.chatExtension("product.dot", roomId: "room")
    private let engine = RecordingChatEngine()
    private let output = RecordingChatInteractorOutput()

    init() {
        engine.store = .product
    }

    @Test func aReactionTogglesThroughTheSource() async {
        let interactor = makeInteractor()

        interactor.toggleReaction(messageId: "m1", emoji: "👍")

        #expect(await nextCall() == .perform(.toggleReaction(messageId: "m1", emoji: "👍")))
    }

    @Test func aRetrySendsTheFailedMessageAgain() async {
        let interactor = makeInteractor()

        interactor.retrySend(messageId: "local:1")

        #expect(await nextCall() == .perform(.retry("local:1")))
    }

    @Test func aRefusedActionShowsTheReason() async {
        engine.actionOutcome = .refused(reason: "Not now")
        let interactor = makeInteractor()

        interactor.toggleReaction(messageId: "m1", emoji: "👍")

        #expect(await nextRefusal() == .refused(reason: "Not now"))
    }

    @Test func aRefusedSendShowsTheReason() async {
        engine.sendError = ChatUserActionRefusal(reason: "Too long")
        let interactor = makeInteractor()

        interactor.send(text: "hello", attachments: nil, replyToMessageId: nil)

        #expect(await nextRefusal() == .refused(reason: "Too long"))
    }

    @Test func aRefusedDeleteShowsADefaultText() async {
        engine.sendError = ChatUserActionRefusal(reason: nil)
        let interactor = makeInteractor()

        interactor.deleteMessage(messageId: "m1")

        #expect(await nextRefusal() == .refused(reason: nil))
    }

    /// The source decides which message is the newest seen, so the interactor gives it every visible id.
    @Test func seenMessagesGoToTheSourceUnfiltered() async {
        let interactor = makeInteractor()

        interactor.readAllBefore(identifier: "m7")

        #expect(await nextCall() == .markAsSeen(["m7"]))
    }

    @Test func aHostStoreRoomKeepsItsReadFilter() async {
        engine.store = .host
        let interactor = makeInteractor()

        // The host-store path marks only unread messages of the model, and the model is empty.
        interactor.readAllBefore(identifier: "m7")
        interactor.jump(to: "after")

        #expect(await nextCall() == .jump("after"))
    }

    /// The "New messages" marker waits for the first page, so a loading window does not fix it.
    @Test func theNewMessagesMarkerWaitsForTheFirstPage() async {
        let state = ChatInteractorState()
        let unread = message("m2", status: .incoming(.new))

        _ = await state.onSnapshot(MessageListSnapshot(messages: [], readiness: .loading))
        let model = await state.onSnapshot(MessageListSnapshot(
            messages: [message("m1", status: .incoming(.seen)), unread],
            readiness: .ready
        ))

        #expect(model.initiallyUnreadMessage == "m2")
    }
}

private extension ChatInteractorUserActionTests {
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

    func nextCall() async -> RecordingChatEngine.Call? {
        for await call in engine.calls {
            return call
        }
        return nil
    }

    func nextRefusal() async -> RecordingChatInteractorOutput.Event? {
        for await event in output.events {
            if case .refused = event {
                return event
            }
        }
        return nil
    }
}
