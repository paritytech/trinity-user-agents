import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The sheet of one participant of a product room (P6.4, scenario S31).
@MainActor
struct ChatParticipantSheetTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()

    @Test func eachKindHasItsHostEntry() {
        let account = Foundation.Data(repeating: 2, count: 32)

        #expect(ChatParticipantSheet.builtIns(for: participant(.litePerson(account: account)), productId: "s.dot")
            == [.copyAddress(ChatParticipantSheet.address(of: account))])
        #expect(ChatParticipantSheet.builtIns(for: participant(.product(productId: "a.dot")), productId: "s.dot")
            == [.openProduct(productId: "a.dot")])
        #expect(ChatParticipantSheet.builtIns(for: participant(.roomProduct), productId: "s.dot")
            == [.openProduct(productId: "s.dot")])
        #expect(ChatParticipantSheet.builtIns(for: participant(.custom(nickname: "G", avatar: nil)), productId: "s.dot")
            .isEmpty)
        #expect(ChatParticipantSheet.address(of: account).hasPrefix("5"))
    }

    @Test func extendBuiltInKeepsTheHostEntryAndCustomDropsIt() {
        let mention = Data.action("act-mention")
        let builtIns: [ChatParticipantSheetEntry] = [.openProduct(productId: "a.dot")]

        #expect(ChatParticipantSheet.entries(builtIns: builtIns, answer: .extendBuiltIn([mention]))
            == [.openProduct(productId: "a.dot"), .product(mention)])
        #expect(ChatParticipantSheet.entries(builtIns: builtIns, answer: .custom([mention])) == [.product(mention)])
        #expect(ChatParticipantSheet.entries(builtIns: builtIns, answer: nil) == builtIns)
    }

    @Test func theSheetAsksTheProductWithTheEntryOfTheParticipant() async throws {
        room.participantMenuAnswer = .success(.init(menu: .extendBuiltIn(actions: [Data.wireAction("act-mention")])))
        let presenter = try makePresenter(participantActions: true)

        let entries = await presenter.participantSheetEntries(for: participant(.product(productId: "a.dot")))

        #expect(entries == [.openProduct(productId: "a.dot"), .product(Data.action("act-mention"))])
        #expect(room.calls.participantMenus.first?.entry == ChatParticipantEntry(
            participantId: "p4",
            participant: .product(productId: "a.dot")
        ))
    }

    @Test func aRoomWithoutParticipantMenusShowsTheHostEntryOnly() async throws {
        let presenter = try makePresenter(participantActions: false)

        let entries = await presenter.participantSheetEntries(for: participant(.product(productId: "a.dot")))

        #expect(entries == [.openProduct(productId: "a.dot")])
        #expect(room.calls.participantMenus.isEmpty)
    }

    @Test func aRefusedQuestionShowsTheHostEntry() async throws {
        room.participantMenuAnswer = .failure(ChatQueryError.timedOut)
        let presenter = try makePresenter(participantActions: true)

        let entries = await presenter.participantSheetEntries(for: participant(.product(productId: "a.dot")))

        #expect(entries == [.openProduct(productId: "a.dot")])
    }

    @Test func aProductEntrySendsParticipantActionTriggered() async throws {
        let presenter = try makePresenter(participantActions: true)

        presenter.select(.product(Data.action("act-mention")), of: participant(.person(personId: 1)))
        await room.waitForEvents(1)

        #expect(room.events == [.participantActionTriggered(participantId: "p4", actionId: "act-mention")])
    }

    @Test func aSelectedParticipantOpensItsSheet() async throws {
        let presenter = try makePresenter(participantActions: false)

        presenter.didSelectParticipant(participant(.product(productId: "a.dot")))
        for _ in 0 ..< 100 where wireframe.sheets.isEmpty {
            try await Task.sleep(for: .milliseconds(5))
        }

        let sheet = try #require(wireframe.sheets.first)
        #expect(sheet.title == "a.dot")
        #expect(sheet.entries == [.openProduct(productId: "a.dot")])

        wireframe.selectSheetEntry?(.openProduct(productId: "a.dot"))
        #expect(wireframe.openedProducts == ["a.dot"])
    }

    /// In a room of the user and one other participant, the menu of that participant is in the header, as the native
    /// person chat shows Block (PARITY-NATIVE-IOS C15).
    @Test func aRoomOfTwoShowsTheMenuOfThePeerInTheHeader() async throws {
        room.participantsAnswer = { _ in
            .success(.page(
                participants: [.init(participantId: "c7", participant: .custom(nickname: "bob", avatar: nil))],
                next: nil
            ))
        }
        room.participantMenuAnswer = .success(.init(menu: .custom(actions: [
            Data.wireAction("block", destructive: true),
            Data.wireAction("note")
        ])))
        let presenter = try makePresenter(participantActions: true)
        for _ in 0 ..< 200 where presenter.peerMenu.isEmpty {
            try await Task.sleep(for: .milliseconds(5))
        }

        #expect(presenter.peerMenu.map(\.action.actionId) == ["note", "block"])
        let header = presenter.peerMenuActions().compactMap { action -> Chat.CustomPeerAction? in
            if case let .custom(custom) = action { custom } else { nil }
        }
        #expect(header.map { $0.titleProvider() } == ["note", "block"])
        #expect(header.map(\.destructive) == [false, true])

        header.last?.handler()
        await room.waitForEvents(1)
        #expect(room.events == [.participantActionTriggered(participantId: "c7", actionId: "block")])
    }

    @Test func aRoomOfMoreThanTwoKeepsItsHeader() async throws {
        room.participantsAnswer = { _ in
            .success(.page(
                participants: [
                    .init(participantId: "c7", participant: .custom(nickname: "bob", avatar: nil)),
                    .init(participantId: "c8", participant: .custom(nickname: "carol", avatar: nil))
                ],
                next: nil
            ))
        }
        room.participantMenuAnswer = .success(.init(menu: .custom(actions: [Data.wireAction("block")])))
        let presenter = try makePresenter(participantActions: true)
        try await Task.sleep(for: .milliseconds(200))

        #expect(presenter.peerMenu.isEmpty)
        #expect(room.calls.participantMenus.isEmpty)
    }
}

private extension ChatParticipantSheetTests {
    func participant(_ kind: ChatRoomParticipant.Kind) -> ChatRoomParticipant {
        ChatRoomParticipant(participantId: "p4", kind: kind)
    }

    func makePresenter(participantActions: Bool) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.update(
            policy: Data.policy(Data.config(participants: true, participantActions: participantActions)),
            messages: []
        )
        return presenter
    }
}
