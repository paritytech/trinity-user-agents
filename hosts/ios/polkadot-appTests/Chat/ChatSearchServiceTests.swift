import CoreData
import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The search of the stored messages (P10.5, 07-LATER §8.2): the mappers write the normalized text of every message
/// kind, and the service finds it without case and diacritics, newest first, a page at a time.
struct ChatSearchServiceTests {
    private let fixture = ProductRoomFixture()

    @Test func theMapperWritesTheNormalizedTextOfEachKind() async throws {
        try await createRoom("lobby")
        try await save("Crème Brûlée", id: "text", at: 1)
        let file = Chat.LocalMessage.Content.ProductFile(
            url: "https://x.y/a.pdf",
            fileName: "Report.pdf",
            mimeType: "application/pdf",
            sizeBytes: 1,
            text: nil
        )
        try await save(.productBody(.file(file)), id: "file", at: 2)
        try await save(.contactAdded, id: "system", at: 3)

        #expect(try await storedTexts() == ["text": "creme brulee", "file": "report.pdf", "system": ""])
    }

    @Test func searchIgnoresCaseAndDiacriticsAndGivesTheNewestFirst() async throws {
        try await createRoom("lobby")
        try await save("Crème brûlée tonight", id: "old", at: 1)
        try await save("no match here", id: "other", at: 2)
        try await save("CREME BRULEE again", id: "new", at: 3)

        let page = try await ChatSearchService(databaseService: fixture.storageFacade.databaseService)
            .searchMessages(matching: "creme Brûlée", in: fixture.chatId("lobby"), after: nil)

        #expect(page.hits.map(\.messageId) == ["new", "old"])
        #expect(page.hits.first?.text == "CREME BRULEE again")
        #expect(page.next == nil)
    }

    @Test func searchPagesFiftyAtATimeAndStaysInTheRoom() async throws {
        try await createRoom("lobby")
        try await createRoom("other")
        for index in 0 ..< 60 {
            try await save("needle \(index)", id: String(format: "m%03d", index), at: UInt64(index))
        }
        try await save("needle elsewhere", id: "elsewhere", at: 100, room: "other")
        let service = ChatSearchService(databaseService: fixture.storageFacade.databaseService)

        let first = try await service.searchMessages(matching: "needle", in: fixture.chatId("lobby"), after: nil)
        let second = try await service.searchMessages(
            matching: "needle",
            in: fixture.chatId("lobby"),
            after: first.next
        )

        #expect(first.hits.count == ChatSearchService.pageSize)
        #expect(first.hits.first?.messageId == "m059")
        #expect(second.hits.map(\.messageId) == (0 ..< 10).reversed().map { String(format: "m%03d", $0) })
        #expect(second.next == nil)
    }

    /// An edit is its own row: a hit on it shows the message that it changed, once. A deleted message is not found.
    @Test func anEditFindsTheEditedMessageAndADeletionHidesIt() async throws {
        try await createRoom("lobby")
        try await save("first draft", id: "draft", at: 1)
        try await save(edit(of: "draft", to: "final draft"), id: "edit-1", at: 2)
        try await save("draft to delete", id: "gone", at: 3)
        try await fixture.storageFacade
            .createRepository(mapper: AnyCoreDataMapper(ChatMessageDeletionMapper()))
            .saveOperation({ [Chat.MessageDeletion(messageId: "gone")] }, { [] })
            .asyncExecute()
        let service = ChatSearchService(databaseService: fixture.storageFacade.databaseService)

        let hits = try await service.searchMessages(matching: "draft", in: fixture.chatId("lobby"), after: nil).hits

        #expect(hits.map(\.messageId) == ["draft"])
        #expect(hits.first?.text == "final draft")
        #expect(try await service.searchMessages(matching: "final", in: fixture.chatId("lobby"), after: nil)
            .hits.map(\.messageId) == ["draft"])
    }

    /// A row that a build before search wrote has no text yet. The first search fills it.
    @Test func theFirstSearchFillsTheTextOfOlderRows() async throws {
        try await createRoom("lobby")
        try await save("written before search", id: "legacy", at: 1)
        try await fixture.storageFacade.databaseService.performWrite { context in
            let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
            try context.fetch(request).forEach { $0.searchableText = nil }
        }

        let hits = try await ChatSearchService(databaseService: fixture.storageFacade.databaseService)
            .searchMessages(matching: "BEFORE", in: fixture.chatId("lobby"), after: nil).hits

        #expect(hits.map(\.messageId) == ["legacy"])
        #expect(try await storedTexts() == ["legacy": "written before search"])
    }

    @Test func theListSearchGoesThroughTheChatsThatItNames() async throws {
        try await createRoom("lobby")
        try await createRoom("source")
        try await save("shared word", id: "a", at: 1)
        try await save("shared word too", id: "b", at: 2, room: "source")

        let hits = try await ChatSearchService(databaseService: fixture.storageFacade.databaseService)
            .searchMessages(matching: "SHARED", in: [fixture.chatId("lobby")], limit: 50)

        #expect(hits.map(\.messageId) == ["a"])
        #expect(hits.first?.chatId == fixture.chatId("lobby"))
    }

    @Test func anEmptyTextFindsNothing() async throws {
        try await createRoom("lobby")
        try await save("anything", id: "a", at: 1)

        let page = try await ChatSearchService(databaseService: fixture.storageFacade.databaseService)
            .searchMessages(matching: "", in: fixture.chatId("lobby"), after: nil)

        #expect(page.hits.isEmpty)
    }
}

private extension ChatSearchServiceTests {
    func createRoom(_ roomId: String) async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: roomId, name: roomId, icon: nil)
    }

    func save(_ text: String, id: String, at timestamp: UInt64, room: String = "lobby") async throws {
        try await save(.text(text), id: id, at: timestamp, room: room)
    }

    func save(
        _ content: Chat.LocalMessage.Content,
        id: String,
        at timestamp: UInt64,
        room: String = "lobby"
    ) async throws {
        try await fixture.save(Chat.LocalMessage(
            messageId: id,
            chatId: fixture.chatId(room),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.delivered),
            timestamp: timestamp,
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        ))
    }

    func edit(of messageId: String, to text: String) -> Chat.LocalMessage.Content {
        .edited(Chat.RemoteMessageContentV1.MessageContent.EditedContent(
            messageId: messageId,
            newContent: .init(text: text, attachments: nil)
        ))
    }

    func storedTexts() async throws -> [String: String] {
        try await fixture.storageFacade.databaseService.performRead { context in
            let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
            let rows = try context.fetch(request)
            return Dictionary(uniqueKeysWithValues: rows.compactMap { row in
                row.messageId.map { ($0, row.searchableText ?? "<nil>") }
            })
        }
    }
}
