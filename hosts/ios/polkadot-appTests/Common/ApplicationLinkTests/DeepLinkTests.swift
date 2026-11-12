@testable import polkadot_app
import Foundation
import Testing

struct DeepLinkParsingTests {
    @Test("Parses deeplink to open chat with preexistence check")
    func parseChatDeeplinkNoForce() {
        performChatDeeplinkTest(chatId: .chatExtension("EchoBot", roomId: nil), force: false)
    }

    @Test("Parses deeplink to open chat without preexistence check")
    func parseChatDeeplinkWithForce() {
        performChatDeeplinkTest(chatId: .chatExtension("EchoBot", roomId: nil), force: true)
    }

    private func performChatDeeplinkTest(chatId: Chat.Id, force: Bool) {
        let url = AppConfig.DeepLink.chat(chatId, force: force)

        let components = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems

        #expect(components?.count == 2)
        #expect(components?[0].name == "id")
        #expect(components?[0].value == chatId.rawRepresentation)
        #expect(components?[1].name == "force")
        #expect(components?[1].value.flatMap(Bool.init) == force)
    }
}
