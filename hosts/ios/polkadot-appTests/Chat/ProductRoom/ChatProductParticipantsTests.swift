import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The participants of a product room (P6.3, scenario S30).
@MainActor
struct ChatProductParticipantsTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()

    // MARK: - Appearance

    @Test func eachKindDrawsItsOwnAppearance() {
        let factory = ChatParticipantAppearanceFactory(myUsername: "alice", productName: "Sample", productId: "s.dot")

        #expect(factory.appearance(of: .init(participantId: "user", kind: .user)).name == "alice")
        #expect(factory.appearance(of: .init(participantId: "product", kind: .roomProduct)).name == "Sample")
        #expect(factory.appearance(of: .init(participantId: "p1", kind: .person(personId: 1))).name == "Person #1")
        #expect(
            factory.appearance(of: .init(participantId: "p2", kind: .litePerson(account: account(0x02)))).name
                == "0x0202…0202"
        )
        #expect(factory.appearance(of: .init(participantId: "p4", kind: .product(productId: "a.dot"))).name == "a.dot")

        let guest = factory.appearance(of: .init(participantId: "p5", kind: .custom(nickname: "Guest 1", avatar: nil)))
        #expect(guest.name == "Guest 1")
        #expect(guest.initials == "G")
    }

    @Test func aParticipantOfTheWireKeepsItsKind() {
        let entries = [
            ChatParticipantEntry(participantId: "p1", participant: .person(personId: 1)),
            ChatParticipantEntry(participantId: "p3", participant: .contact(handle: ContactHandle(bytes: account(3)))),
            ChatParticipantEntry(participantId: "p5", participant: .custom(nickname: "Guest 1", avatar: nil))
        ]

        let participants = entries.map(ChatRoomParticipant.init)

        #expect(participants.map(\.kind) == [
            .person(personId: 1),
            .contact(handle: account(3)),
            .custom(nickname: "Guest 1", avatar: nil)
        ])
        #expect(participants.compactMap(\.wireEntry) == entries)
    }

    // MARK: - Directory

    @Test func theReservedIdsResolveWithoutAQuestion() async {
        let directory = makeDirectory()

        directory.resolve(["user", "product"])

        #expect(directory.participant("user")?.kind == .user)
        #expect(directory.participant("product")?.kind == .roomProduct)
        try? await Task.sleep(for: .milliseconds(20))
        #expect(room.calls.participants.isEmpty)
    }

    @Test func idsGoInBatchesOfAHundredAndEachIdIsAskedOnce() async throws {
        let directory = makeDirectory()
        room.participantsAnswer = { query in
            guard case let .byIds(ids) = query else { return .failure(ChatQueryError.noAnswer) }
            return .success(.resolved(participants: ids
                    .map { .init(participantId: $0, participant: .person(personId: 7)) }))
        }

        let ids = (1 ... 150).map { "p\($0)" }
        directory.resolve(ids)
        directory.resolve(ids)
        try await waitUntil { directory.participant("p150") != nil && directory.participant("p1") != nil }

        let batches = room.calls.participants.compactMap { request -> [String]? in
            if case let .byIds(ids) = request.query { ids } else { nil }
        }
        #expect(batches.map(\.count).sorted() == [50, 100])
        #expect(Set(batches.flatMap { $0 }).count == 150)
    }

    @Test func theCountComesOnceAndAPageFillsTheDirectory() async throws {
        let directory = makeDirectory()
        room.participantsAnswer = { query in
            switch query {
            case .count: .success(.count(count: 104))
            case .page: .success(.page(
                    participants: [.init(participantId: "p1", participant: .person(personId: 1))],
                    next: "100"
                ))
            case .byIds: .failure(ChatQueryError.noAnswer)
            }
        }

        directory.loadCount()
        directory.loadCount()
        try await waitUntil { directory.count != nil }
        let page = try await directory.page(after: nil)

        #expect(directory.count == 104)
        #expect(page.next == "100")
        #expect(directory.participant("p1")?.kind == .person(personId: 1))
        #expect(room.calls.participants.filter { $0.query == .count }.count == 1)
        #expect(room.calls.participants.last?.query == .page(cursor: nil, pageSize: 100))
    }

    // MARK: - Room screen

    @Test func theHeaderShowsTheCountOfParticipants() async throws {
        room.participantsAnswer = { _ in .success(.count(count: 9)) }
        let presenter = try makePresenter(participants: true)

        #expect(presenter.headerSubtitle(default: "A room") == "A room")
        try await waitUntil { presenter.participants.count != nil }

        #expect(presenter.headerSubtitle(default: "A room") == "9 participants")
    }

    /// `/participants <n>` declares the participants again, so a new configuration asks the count again.
    @Test func aNewConfigurationAsksTheCountAgain() async throws {
        var count: UInt32 = 4
        room.participantsAnswer = { _ in .success(.count(count: count)) }
        let presenter = try makePresenter(participants: true)
        try await waitUntil { presenter.participants.count == 4 }

        count = 10
        let declarations = Chat.RoomDeclarations(
            description: nil,
            avatar: nil,
            config: Data.config(participants: true, participantActions: true),
            configGeneration: 2,
            preview: .hostDefault,
            toolbar: nil,
            suggestions: nil,
            commands: nil,
            footerConfig: nil
        )
        presenter.update(policy: ChatRoomPolicy(footer: nil, declarations: declarations), messages: [])
        try await waitUntil { presenter.participants.count == 10 }

        #expect(presenter.headerSubtitle(default: nil) == "10 participants")
    }

    /// P7.3: a count that the product does not give in the lookup time (02-DESIGN §3.6) keeps the host default,
    /// the description of the room.
    @Test func aCountThatTimesOutKeepsTheDescription() async throws {
        room.participantsAnswer = { _ in .failure(ChatQueryError.timedOut) }
        let presenter = try makePresenter(participants: true)
        try await waitUntil { !room.calls.participants.isEmpty }
        try await Task.sleep(for: .milliseconds(20))

        #expect(presenter.participants.count == nil)
        #expect(presenter.headerSubtitle(default: "A room") == "A room")
    }

    @Test func aRoomWithoutParticipantsAsksNothingAndKeepsTheDescription() async throws {
        let presenter = try makePresenter(participants: false)
        var message = Data.message("m1")
        message.authorParticipantId = "p1"

        #expect(presenter.author(of: message) == nil)
        #expect(presenter.headerSubtitle(default: "A room") == "A room")
        #expect(presenter.participantsButton() == nil)
        try await Task.sleep(for: .milliseconds(20))
        #expect(room.calls.participants.isEmpty)
    }

    @Test func theAuthorOfAMessageShowsItsResolvedName() async throws {
        room.participantsAnswer = { query in
            switch query {
            case .byIds: .success(.resolved(participants: [.init(
                    participantId: "p1",
                    participant: .person(personId: 1)
                )]))
            default: .failure(ChatQueryError.refused(.notAvailable))
            }
        }
        let presenter = try makePresenter(participants: true)
        var message = Data.message("m1")
        message.authorParticipantId = "p1"

        #expect(presenter.author(of: message)?.name == "p1")
        try await waitUntil { presenter.participants.participant("p1") != nil }

        #expect(presenter.author(of: message)?.name == "Person #1")
        #expect(presenter.participantsButton() != nil)
    }

    // MARK: - List

    @Test func theListPagesUntilTheProductEndsIt() async throws {
        room.participantsAnswer = { query in
            guard case let .page(cursor, _) = query else { return .failure(ChatQueryError.noAnswer) }
            let entry = ChatParticipantEntry(participantId: cursor ?? "first", participant: .person(personId: 1))
            return .success(.page(participants: [entry], next: cursor == nil ? "second" : nil))
        }
        let interactor = ChatParticipantsInteractor(directory: makeDirectory())
        let output = RecordingParticipantsOutput()
        interactor.presenter = output

        interactor.loadNextPage()
        try await waitUntil { output.pages.count == 1 }
        interactor.loadNextPage()
        try await waitUntil { output.pages.count == 2 }
        interactor.loadNextPage()
        try await Task.sleep(for: .milliseconds(20))

        #expect(output.pages.map { $0.map(\.participantId) } == [["first"], ["second"]])
        #expect(output.lastFlags == [false, true])
        #expect(room.calls.participants.count == 2)
    }
}

@MainActor
private final class RecordingParticipantsOutput: ChatParticipantsInteractorOutputProtocol {
    private(set) var pages: [[ChatRoomParticipant]] = []
    private(set) var lastFlags: [Bool] = []
    private(set) var failures = 0

    func didReceive(page: [ChatRoomParticipant], isLast: Bool) {
        pages.append(page)
        lastFlags.append(isLast)
    }

    func didFailToLoadPage() {
        failures += 1
    }
}

private extension ChatProductParticipantsTests {
    func account(_ byte: UInt8) -> Foundation.Data {
        Foundation.Data(repeating: byte, count: 32)
    }

    func makeDirectory() -> ChatParticipantDirectory {
        ChatParticipantDirectory(roomId: "lobby", room: room, logger: StubLogger())
    }

    func makePresenter(participants: Bool) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.update(
            policy: Data.policy(Data.config(participants: participants, participantActions: participants)),
            messages: []
        )
        return presenter
    }

    func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0 ..< 200 where !condition() {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(condition())
    }
}
