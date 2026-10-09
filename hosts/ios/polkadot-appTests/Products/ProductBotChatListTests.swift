import Foundation
import PolkadotUI
import Products
import Testing
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

/// A product's chat row is drawn from its manifest, the way its name already is.
@Suite("Product bot in the chat list")
struct ProductBotChatListTests {
    @Test
    func aWidgetPreviewsAsTheManifestDescription() {
        let bot = makeBot(description: "Play the weekly game with friends")

        #expect(bot.messageDecoder.previewString(data: Data()) == "Play the weekly game with friends")
    }

    /// A manifest always has a description, but a product with no manifest has none.
    @Test
    func aWidgetOfAProductWithoutADescriptionPreviewsAsAWidget() {
        let bot = makeBot(description: nil)

        #expect(bot.messageDecoder.previewString(data: Data()) == String(localized: .Common.productWidgetMessage))
    }

    /// The list loads the image by domain, so the row shows the manifest icon instead of a letter.
    @Test
    func theRowIconIsTheProductManifestIcon() {
        let bot = makeBot(description: nil)

        #expect(bot.peerMetadata.icon == .product(domain: "dim2.paseo"))
    }
}

private extension ProductBotChatListTests {
    func makeBot(description: String?) -> ProductBot {
        ProductBot(
            product: Product(id: "dim2.paseo", name: "Jollity"),
            description: description,
            runtime: IdleChatRuntime(),
            resolveImage: WidgetImageResolver { _ in nil }
        )
    }
}

private final class IdleChatRuntime: ChatRuntimeProtocol, @unchecked Sendable {
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
    func dispose() async {}
}
