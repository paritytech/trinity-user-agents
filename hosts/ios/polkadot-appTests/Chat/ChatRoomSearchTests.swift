import Foundation
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The in-room search (P10.5, 07-LATER §8.2): the product of a product-source room answers `search_query` 30 hits a
/// page, a refusal shows "not available", and the field waits for a pause in the typing.
@MainActor
struct ChatRoomSearchTests {
    private let chatId = Chat.Id.chatExtension("game.dot", roomId: "source")

    @Test func theProductIsAskedThirtyHitsAPageWithItsCursor() async throws {
        let product = ScriptedProductSearch(answers: [
            .success(ProductChatSearchQueryItem(hits: [message("m2", by: "user"), message("m1", by: "p1")], next: "2"))
        ])
        let search = ProductChatRoomSearch(chatId: chatId, roomId: "source", searching: product, logger: StubLogger())

        let page = try await search.page(matching: "needle", after: .product("7"))

        #expect(product.requests == [
            ProductChatSearchQueryRequest(roomId: "source", text: "needle", cursor: "7", pageSize: 30)
        ])
        #expect(page.hits.map(\.messageId) == ["m2", "m1"])
        #expect(page.hits.map(\.isOutgoing) == [true, false])
        #expect(page.hits.map(\.authorParticipantId) == [nil, "p1"])
        #expect(page.hits.first?.text == "text of m2")
        #expect(page.next == .product("2"))
    }

    @Test func aRefusalOrATimeOutIsNotAvailable() async throws {
        let product = ScriptedProductSearch(answers: [
            .failure(ChatQueryError.refused(.notAvailable)),
            .failure(ChatQueryError.timedOut)
        ])
        let search = ProductChatRoomSearch(chatId: chatId, roomId: "source", searching: product, logger: StubLogger())

        for _ in 0 ..< 2 {
            await #expect(throws: ChatRoomSearchError.unavailable) {
                try await search.page(matching: "needle", after: nil)
            }
        }
    }

    /// Typing asks once, after the pause, with the last text. An older page adds its hits after the first page.
    @Test func theFieldAsksOnceAfterThePauseAndPagesOnDemand() async throws {
        let source = ScriptedRoomSearch(pages: [
            "nee": ChatRoomSearchPage(hits: [hit("a")], next: nil),
            "needle": ChatRoomSearchPage(hits: [hit("b")], next: .product("1"))
        ], older: ChatRoomSearchPage(hits: [hit("c")], next: nil))
        let output = RecordingSearchOutput()
        let interactor = ChatSearchInteractor(source: source)
        interactor.presenter = output

        interactor.search(text: "nee")
        interactor.search(text: "needle")
        try await output.waitForHits()
        interactor.loadMore()
        try await output.waitForHits(count: 2)

        #expect(source.asked == ["needle", "needle@1"])
        #expect(output.hits.last?.map(\.messageId) == ["b", "c"])
        #expect(output.hasMore.last == false)
    }

    @Test func aFailedSearchShowsNotAvailableAndAnEmptyFieldClears() async throws {
        let source = ScriptedRoomSearch(pages: [:], older: nil)
        let output = RecordingSearchOutput()
        let interactor = ChatSearchInteractor(source: source)
        interactor.presenter = output

        interactor.search(text: "x")
        try await output.waitForFailure()
        interactor.search(text: "  ")

        #expect(output.failures == [.unavailable])
        #expect(output.cleared == 1)
    }

    @Test func theExcerptKeepsTheLineOfTheMatchAndMarksIt() {
        let excerpt = ChatSearchExcerpt(text: "first line\nthe Crème brûlée line", query: "creme")

        #expect(excerpt.line == "the Crème brûlée line")
        #expect(excerpt.match == NSRange(location: 4, length: 5))
    }

    @Test func aLongLineIsCutBeforeTheMatch() {
        let text = String(repeating: "x", count: 80) + " needle"

        let excerpt = ChatSearchExcerpt(text: text, query: "needle")

        #expect(excerpt.line.hasPrefix("…"))
        #expect(excerpt.match.map { (excerpt.line as NSString).substring(with: $0) } == "needle")
    }

    /// A host-store room always offers search. A product-source room offers it only when it declares `search`.
    @Test func theHeaderOffersSearchWhereTheRoomCanBeSearched() {
        var source = ChatRoomPolicy.defaults
        source = ChatRoomPolicy(
            messageStore: .product,
            listPreview: source.listPreview,
            listBadge: source.listBadge,
            listOrder: source.listOrder,
            reportsSeen: false,
            messageMenu: source.messageMenu,
            participants: source.participants,
            offersReactionDetails: false,
            toolbar: [],
            footer: source.footer,
            placeholder: nil,
            hostAppliesUserActions: false
        )
        var declared = source
        declared.features = Chat.RoomFeatures(
            preimageAttachments: false, search: true, calls: nil, readAloud: false
        )

        #expect(ChatRoomPolicy.defaults.offersSearch)
        #expect(!source.offersSearch)
        #expect(declared.offersSearch)
    }
}

private extension ChatRoomSearchTests {
    func message(_ id: String, by participantId: String) -> ChatMessage {
        ChatMessage(
            messageId: id,
            origin: ChatMessageOrigin(participantId: participantId),
            timestamp: 1_000,
            body: .text(text: "text of \(id)"),
            status: .sent,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
    }

    func hit(_ id: String) -> ChatSearchHit {
        ChatSearchHit(
            chatId: chatId,
            messageId: id,
            timestamp: 1,
            text: id,
            isOutgoing: false,
            authorParticipantId: nil
        )
    }
}

private final class ScriptedProductSearch: ProductChatSearching, @unchecked Sendable {
    private let state: OSAllocatedUnfairLock<(
        answers: [Result<ProductChatSearchQueryItem, Error>],
        requests: [ProductChatSearchQueryRequest]
    )>

    init(answers: [Result<ProductChatSearchQueryItem, Error>]) {
        state = OSAllocatedUnfairLock(initialState: (answers, []))
    }

    var requests: [ProductChatSearchQueryRequest] {
        state.withLock { $0.requests }
    }

    func searchPage(_ request: ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem {
        let answer = state.withLock { state in
            state.requests.append(request)
            return state.answers.removeFirst()
        }
        return try answer.get()
    }
}

private final class ScriptedRoomSearch: ChatRoomSearching, @unchecked Sendable {
    let pages: [String: ChatRoomSearchPage]
    let older: ChatRoomSearchPage?
    private let log = OSAllocatedUnfairLock<[String]>(initialState: [])

    init(pages: [String: ChatRoomSearchPage], older: ChatRoomSearchPage?) {
        self.pages = pages
        self.older = older
    }

    var typingPause: Duration {
        .milliseconds(30)
    }

    var asked: [String] {
        log.withLock { $0 }
    }

    func page(matching text: String, after cursor: ChatRoomSearchCursor?) async throws -> ChatRoomSearchPage {
        if case let .product(next) = cursor {
            log.withLock { $0.append("\(text)@\(next)") }
            return try older.orThrow(ChatRoomSearchError.unavailable)
        }
        log.withLock { $0.append(text) }
        return try pages[text].orThrow(ChatRoomSearchError.unavailable)
    }
}

@MainActor
private final class RecordingSearchOutput: ChatSearchInteractorOutputProtocol {
    private(set) var hits: [[ChatSearchHit]] = []
    private(set) var hasMore: [Bool] = []
    private(set) var failures: [ChatRoomSearchError] = []
    private(set) var cleared = 0

    func didStartSearch() {}

    func didReceive(hits: [ChatSearchHit], hasMore: Bool) {
        self.hits.append(hits)
        self.hasMore.append(hasMore)
    }

    func didClear() {
        cleared += 1
    }

    func didFail(_ error: ChatRoomSearchError) {
        failures.append(error)
    }

    func waitForHits(count: Int = 1) async throws {
        try await waitUntil { self.hits.count >= count }
    }

    func waitForFailure() async throws {
        try await waitUntil { !self.failures.isEmpty }
    }

    private func waitUntil(_ condition: () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(30)
        while !condition() {
            guard ContinuousClock.now < deadline else {
                Issue.record("the search did not answer in time")
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}

private extension Optional {
    func orThrow(_ error: Error) throws -> Wrapped {
        guard let self else { throw error }
        return self
    }
}
