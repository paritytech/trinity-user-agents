import Foundation
import KeyDerivation
import PolkadotUI
import Products
import Testing
import UIKitExt
@testable import polkadot_app

@Suite("Product bot")
struct ProductBotTests {
    /// A host-placed product is removable by nobody, so its rooms offer no way to leave.
    @Test
    func aHostPlacedProductOffersNoActions() {
        let bot = makeBot(productId: BuiltInProduct.dim2(for: "paseo"))

        #expect(bot.peerMetadata.moreActions == [])
    }

    @Test
    func anyOtherProductCanBeLeft() {
        let bot = makeBot(productId: "coinflip.paseo")

        #expect(bot.peerMetadata.moreActions == [.leaveChat])
    }
}

private func makeBot(productId: String) -> ProductBot {
    ProductBot(
        product: Product(id: productId, name: "Any"),
        description: nil,
        runtime: IdleChatRuntime(),
        resolveImage: WidgetImageResolver { _ in nil }
    )
}

private final class IdleChatRuntime: ChatRuntimeProtocol {
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

    @MainActor
    func attach(presentationView _: ControllerBackedProtocol) {}

    func dispose() async {}
}
