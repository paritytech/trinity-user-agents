import Foundation
import Testing
import UIKit
@testable import polkadot_app

/// The Chat Extension v2 additions on the room screen (07-LATER §5, §9; scenarios S53 and S67).
@MainActor
struct ChatRoomAdditionsScreenTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
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
}

private extension ChatRoomAdditionsScreenTests {
    func makePresenter(
        readAloud: Bool = false
    ) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            additions: doubles.dependencies(),
            logger: StubLogger()
        ))
        presenter.update(policy: policy(readAloud: readAloud), messages: [])
        return presenter
    }

    func policy(readAloud: Bool = false) -> ChatRoomPolicy {
        var policy = ChatRoomPolicy(footer: nil, declarations: Data.declarations())
        policy.features = Chat.RoomFeatures(preimageAttachments: false, search: false, calls: nil, readAloud: readAloud)
        return policy
    }

    func persona(_ botId: String, name: String) -> Chat.ProductPersona {
        Chat.ProductPersona(productId: Data.productId, botId: botId, name: name, icon: nil)
    }

    func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0 ..< 400 where !condition() {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(condition())
    }
}
