import Foundation
import Testing
@testable import polkadot_app

/// The services of the room screen for the Chat Extension v2 additions (07-LATER §5, §9): personas and read aloud.
struct ChatRoomAdditionsServiceTests {
    private let fixture = ProductRoomFixture()

    // MARK: - Personas

    @Test func theRoomScreenFollowsThePersonasOfItsProduct() async throws {
        let store = ChatProductPersonaStore(storageFacade: fixture.storageFacade)
        var updates = store.personaUpdates(of: fixture.productId).makeAsyncIterator()
        #expect(try await updates.next() == [])

        _ = try await store.register(persona("helper", name: "Helper"))

        #expect(try await updates.next()?.map(\.name) == ["Helper"])
    }

    @Test func aPersonaIdIsBotAndTheBotId() {
        #expect(Chat.ProductPersona.botId(ofParticipant: "bot:helper") == "helper")
        #expect(Chat.ProductPersona.botId(ofParticipant: "bot:") == nil)
        #expect(Chat.ProductPersona.botId(ofParticipant: "p1") == nil)
        #expect(persona("helper", name: "Helper").participantId == "bot:helper")
    }

    // MARK: - Read aloud

    @Test func readAloudSpeaksMarkdownWithoutItsMarks() {
        let markdown = "# Rich text sample\n\nA paragraph with **bold**, *italic* and `code`.\n\n"
            + "- first item\n> A quote"

        #expect(ChatReadAloudService.plainText(fromMarkdown: markdown)
            == "Rich text sample\n\nA paragraph with bold, italic and code.\n\nfirst item\nA quote")
        #expect(ChatReadAloudService.plainText(fromMarkdown: "plain") == "plain")
    }
}

private extension ChatRoomAdditionsServiceTests {
    func persona(_ botId: String, name: String) -> Chat.ProductPersona {
        Chat.ProductPersona(productId: fixture.productId, botId: botId, name: name, icon: nil)
    }
}
