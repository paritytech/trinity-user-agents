import Foundation
import Testing
import UIKit
@testable import polkadot_app

/// The Chat Extension v2 additions on the room screen (07-LATER §5, §9; scenarios S53, S64 to S67).
@MainActor
struct ChatRoomAdditionsScreenTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let activity = ChatRoomActivityStore()
    private let doubles = ChatRoomAdditionsDoubles()

    // MARK: - Personas (S53)

    @Test func aPersonaMessageShowsTheNameOfThePersonaWithoutAQuestion() async throws {
        let presenter = try makePresenter()
        var message = Data.message("m1")
        message.authorParticipantId = "bot:helper"

        doubles.personas.push([persona("helper", name: "Helper")])
        try await waitUntil { presenter.persona(of: "bot:helper") != nil }

        #expect(presenter.author(of: message)?.name == "Helper")
        #expect(presenter.author(of: message)?.participantId == "bot:helper")
        try await Task.sleep(for: .milliseconds(20))
        #expect(room.calls.participants.isEmpty)
    }

    @Test func anUnknownPersonaHasNoAuthorInARoomWithoutParticipants() throws {
        let presenter = try makePresenter()
        var message = Data.message("m1")
        message.authorParticipantId = "bot:ghost"

        #expect(presenter.author(of: message) == nil)
    }

    @Test func theActivityLineNamesThePersonaThatItIsAbout() async throws {
        let presenter = try makePresenter()

        activity.set(.init(text: "Sample is typing", participantId: "bot:helper"), for: Data.chatId)
        try await waitUntil { presenter.accessory.activityText == "Sample is typing" }

        doubles.personas.push([persona("helper", name: "Helper Two")])
        try await waitUntil { presenter.accessory.activityText == "Helper Two: Sample is typing" }
    }

    // MARK: - Branches (S65)

    @Test func anArrowSendsBranchSelectedAndTheAnswerOfTheProductEndsTheWait() async throws {
        let presenter = try makePresenter()
        doubles.meta.push(["m1": meta(branch: .init(index: 1, count: 3))])
        try await waitUntil { presenter.meta(of: "m1")?.branch != nil }

        presenter.selectBranch(of: "m1", index: 2)

        let selected = RecordingRoomEventPublisher.Published(
            event: .branchSelected(messageId: "m1", index: 2),
            productId: Data.productId,
            roomId: "lobby"
        )
        #expect(doubles.events.events == [selected])
        #expect(presenter.pendingBranchIndex(of: "m1") == 2)

        presenter.selectBranch(of: "m1", index: 3)
        #expect(doubles.events.events.count == 1)

        doubles.meta.push(["m1": meta(branch: .init(index: 2, count: 3))])
        try await waitUntil { presenter.pendingBranchIndex(of: "m1") == nil }
        #expect(presenter.meta(of: "m1")?.branch == .init(index: 2, count: 3))
    }

    @Test func withoutAnAnswerTheControlGoesBack() async throws {
        let presenter = try makePresenter(branchTimeout: .milliseconds(50))
        doubles.meta.push(["m1": meta(branch: .init(index: 1, count: 3))])
        try await waitUntil { presenter.meta(of: "m1")?.branch != nil }

        presenter.selectBranch(of: "m1", index: 2)
        #expect(presenter.pendingBranchIndex(of: "m1") == 2)

        try await waitUntil { presenter.pendingBranchIndex(of: "m1") == nil }
        #expect(presenter.meta(of: "m1")?.branch == .init(index: 1, count: 3))
    }

    // MARK: - Reasoning rows (S66)

    @Test func aReasoningRowThinksWhileItsMessageIsPreparingAndThenExpandsOnATap() async throws {
        let presenter = try makePresenter()

        doubles.preparing.setPreparing(true, productMessageId: "m1", chatId: Data.chatId)
        try await waitUntil { presenter.isThinking("m1") }

        doubles.preparing.setPreparing(false, productMessageId: "m1", chatId: Data.chatId)
        try await waitUntil { !presenter.isThinking("m1") }
        #expect(!presenter.isReasoningExpanded("m1"))

        presenter.toggleReasoning(of: "m1")
        #expect(presenter.isReasoningExpanded("m1"))
        presenter.toggleReasoning(of: "m1")
        #expect(!presenter.isReasoningExpanded("m1"))
    }

    // MARK: - Read aloud (S67)

    @Test func readAloudNeedsTheDeclarationOfTheRoom() throws {
        let presenter = try makePresenter()
        #expect(!presenter.offersReadAloud)

        presenter.readAloud("hello", messageId: "m1")
        #expect(doubles.readAloud.spoken.isEmpty)

        presenter.update(policy: policy(readAloud: true), messages: [])
        #expect(presenter.offersReadAloud)
    }

    @Test func readAloudSpeaksMarkdownAsPlainTextAndStopsWhenTheUserLeaves() throws {
        let presenter = try makePresenter(readAloud: true)
        presenter.screenWillAppear()

        presenter.readAloud("# Title\n\nSome **bold** and `code`.", messageId: "m1")

        #expect(doubles.readAloud.spoken == ["Title\n\nSome bold and code."])
        #expect(presenter.accessory.readAloudStopTitle != nil)

        presenter.screenWillDisappear()

        #expect(doubles.readAloud.stops == [Data.chatId])
        #expect(presenter.accessory.readAloudStopTitle == nil)
    }

    // MARK: - Threads (S64)

    @Test func theChipOpensTheThreadWithTheRootAndTheParentShowsTheRootOnReturn() throws {
        let presenter = try makePresenter()
        let root = Data.message("m1", content: .text("The root"))
        presenter.update(policy: policy(), messages: [root])

        presenter.openThread(summary(), rootMessageId: "m1")

        let threadChatId = Chat.Id.chatExtension(Data.productId, roomId: "thread-lobby-m1")
        #expect(wireframe.shownThreads == [threadChatId])
        #expect(doubles.threadRoots.handedOver.map(\.root.text) == ["The root"])
        #expect(doubles.threadRoots.handedOver.map(\.threadChatId) == [threadChatId])
        #expect(presenter.additions.returnToMessageId == "m1")

        presenter.screenWillAppear()

        #expect(presenter.additions.returnToMessageId == nil)
    }

    @Test func aThreadRoomShowsThreadAndTheParentAndPutsTheParentBelowIt() async throws {
        let presenter = try makePresenter(
            chatId: .chatExtension(Data.productId, roomId: "thread-lobby-m1"),
            threadOf: .init(roomId: "lobby", messageId: "m1")
        )

        try await waitUntil { presenter.additions.threadRoot != nil }
        #expect(presenter.threadHeader()?.title == "Thread")
        #expect(presenter.threadHeader()?.subtitle == "Lobby")
        #expect(presenter.threadRootConfiguration() == .init(author: "Game", text: "The root"))

        presenter.screenWillAppear()
        try await waitUntil { !wireframe.placedParents.isEmpty }

        #expect(wireframe.placedParents.map(\.chatId) == [Data.chatId])
        #expect(wireframe.placedParents.map(\.rootMessageId) == ["m1"])
    }

    @Test func aRoomThatIsNotAThreadHasNoThreadHeader() throws {
        let presenter = try makePresenter()

        #expect(presenter.threadHeader() == nil)
        #expect(presenter.threadRootConfiguration() == nil)
    }
}

private extension ChatRoomAdditionsScreenTests {
    func makePresenter(
        chatId: Chat.Id = Data.chatId,
        threadOf: Chat.ThreadParent? = nil,
        readAloud: Bool = false,
        branchTimeout: Duration = .seconds(5)
    ) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            activityStore: activity,
            additions: doubles.dependencies(branchTimeout: branchTimeout),
            logger: StubLogger()
        ))
        presenter.update(policy: policy(threadOf: threadOf, readAloud: readAloud), messages: [])
        return presenter
    }

    func policy(threadOf: Chat.ThreadParent? = nil, readAloud: Bool = false) -> ChatRoomPolicy {
        var policy = ChatRoomPolicy(footer: nil, declarations: Data.declarations())
        policy.features = Chat.RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            threadOf: threadOf,
            readAloud: readAloud
        )
        return policy
    }

    func persona(_ botId: String, name: String) -> Chat.ProductPersona {
        Chat.ProductPersona(productId: Data.productId, botId: botId, name: name, icon: nil)
    }

    func meta(branch: Chat.BranchPosition) -> Chat.MessageMeta {
        Chat.MessageMeta(thread: nil, branch: branch, reasoning: nil)
    }

    func summary() -> Chat.ThreadSummary {
        Chat.ThreadSummary(
            roomId: "thread-lobby-m1",
            replyCount: 3,
            lastReplyAt: 1_767_225_600_000,
            participantIds: ["p1", "p2", "bot:helper"]
        )
    }

    func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0 ..< 400 where !condition() {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(condition())
    }
}
