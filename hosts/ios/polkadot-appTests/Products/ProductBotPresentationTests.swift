import Foundation
import Products
import Testing
import UIKitExt
@testable import polkadot_app

@Suite("Product bot presentation")
struct ProductBotPresentationTests {
    private func bot(showsTextInput: Bool) -> ProductBot {
        ProductBot(
            product: Product(id: "jollity.dot", name: "Jollity"),
            presentation: .init(showsTextInput: showsTextInput),
            runtime: IdleChatRuntime(),
            logger: StubLogger()
        )
    }

    /// The worker manifest is the product's only say over its rooms, and both chat
    /// runtimes build this bot, so this is where the setting reaches the screen.
    @Test func aWorkerThatTurnsOffTheTextInputGetsNone() {
        #expect(bot(showsTextInput: false).peerMetadata.input == .empty)
    }

    @Test func aWorkerThatSaysNothingKeepsTheTextInput() {
        #expect(bot(showsTextInput: true).peerMetadata.input.isInputField)
    }
}

private final class IdleChatRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    func dispose() async {}

    func start(messagingSupport _: ProductsNativeApi.MessagingSupport) async throws {}

    func onUserMessage(text _: String, roomId _: String?) async throws {}

    func renderMessage(
        roomId _: String?,
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func dispatchEvent(
        roomId _: String?,
        messageId _: String,
        messageType _: String?,
        actionId _: String,
        payload _: String?
    ) async {}

    @MainActor func attach(presentationView _: ControllerBackedProtocol) {}
}
