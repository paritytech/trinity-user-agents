import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

struct ChatCommandTests {
    @Test func aSlashAndANameMakeACommand() throws {
        let command = try #require(Chat.Command(text: "/help"))

        #expect(command == Chat.Command(name: "help", arguments: ""))
    }

    @Test func theTextAfterTheNameIsTheArguments() throws {
        let command = try #require(Chat.Command(text: "  /roll   2d6 with advantage \n"))

        #expect(command.name == "roll")
        #expect(command.arguments == "2d6 with advantage")
    }

    @Test(arguments: ["hello", "/", "/ help", "/path/to/file", "a /help", ""])
    func otherTextStaysAMessage(text: String) {
        #expect(Chat.Command(text: text) == nil)
    }

    @Test func aNameLongerThanAnIdentifierStaysAMessage() {
        let name = String(repeating: "a", count: Chat.Command.maxNameBytes + 1)

        #expect(Chat.Command(text: "/\(name)") == nil)
    }
}

/// The room screen sends `/name arguments` as a command and posts nothing when the room takes it.
struct ChatInteractorCommandTests {
    private let chatId = Chat.Id.chatExtension("product.dot", roomId: "room")
    private let engine = RecordingChatEngine()

    @Test func aCommandTheRoomTakesIsNotPosted() async throws {
        let interactor = makeInteractor()

        interactor.send(text: "/roll 2d6", attachments: nil, replyToMessageId: nil)
        interactor.send(text: "after", attachments: nil, replyToMessageId: nil)

        let calls = await firstCalls(2)
        #expect(calls.contains(.sendCommand(Chat.Command(name: "roll", arguments: "2d6"))))
        #expect(calls.contains(.sendUserMessage(.text("after"))))
        #expect(!calls.contains(.sendUserMessage(.text("/roll 2d6"))))
    }

    @Test func aCommandTheRoomRefusesIsPostedAsText() async throws {
        engine.takesCommands = false
        let interactor = makeInteractor()

        interactor.send(text: "/roll 2d6", attachments: nil, replyToMessageId: nil)

        let calls = await firstCalls(2)
        #expect(calls == [
            .sendCommand(Chat.Command(name: "roll", arguments: "2d6")),
            .sendUserMessage(.text("/roll 2d6"))
        ])
    }

    @Test func aReplyIsNeverACommand() async throws {
        let interactor = makeInteractor()

        interactor.send(text: "/roll", attachments: nil, replyToMessageId: "original")

        let calls = await firstCalls(1)
        guard case let .sendUserMessage(.reply(reply)) = calls.first else {
            Issue.record("a reply must be posted as a reply")
            return
        }
        #expect(reply.ownContent.text == "/roll")
    }
}

private extension ChatInteractorCommandTests {
    func makeInteractor() -> ChatInteractor {
        let reactions = ChatReactionRepository(
            repository: AnyDataProviderRepository(InMemoryDataProviderRepository<Chat.MessageReaction>())
        )
        return ChatInteractor(
            chatId: chatId,
            engine: engine,
            reactionRepository: reactions,
            foregroundVisibilityReporter: nil,
            notificationsCleaner: StubPushNotificationsCleaner()
        )
    }

    func firstCalls(_ count: Int) async -> [RecordingChatEngine.Call] {
        var calls: [RecordingChatEngine.Call] = []
        for await call in engine.calls {
            calls.append(call)
            if calls.count == count { break }
        }
        return calls
    }
}
