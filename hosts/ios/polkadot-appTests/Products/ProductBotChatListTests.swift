import Foundation
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

    /// With the manifest icon in hand the row draws it instead of a letter.
    @Test
    func theRowIconIsTheProductManifestIcon() async {
        let bot = makeBot(description: nil, icon: manifestIcon)

        await bot.loadIcon()

        #expect(bot.peerMetadata.icon == .image(manifestIcon))
    }
}

private extension ProductBotChatListTests {
    var manifestIcon: Data {
        Data([0x89, 0x50, 0x4E, 0x47])
    }

    func makeBot(description: String?, icon: Data? = nil) -> ProductBot {
        ProductBot(
            product: Product(id: "dim2.paseo", name: "Jollity"),
            productDescription: description,
            iconLoader: StubIconLoader(icon: icon),
            runtime: IdleChatRuntime()
        )
    }
}

private struct StubIconLoader: ProductIconLoading {
    let icon: Data?

    func loadIcon(for _: ProductId) async -> Data? {
        icon
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
