import struct TrUAPIHost.ProductId
import Foundation
import KeyDerivation
import PolkadotUI
import Products
import Testing
@testable import polkadot_app

@Suite("Product bot")
struct ProductBotTests {
    /// A host-placed product is removable by nobody, so its rooms offer no way to leave.
    @Test
    func aHostPlacedProductOffersNoActions() throws {
        let bot = try makeBot(productId: BuiltInProduct.dim2(for: "paseo"))

        #expect(bot.peerMetadata.moreActions == [])
    }

    @Test
    func anyOtherProductCanBeLeft() throws {
        let bot = try makeBot(productId: ProductId("coinflip.paseo"))

        #expect(bot.peerMetadata.moreActions == [.leaveChat])
    }
}

private func makeBot(productId: ProductId) -> ProductBot {
    ProductBot(
        product: Product(id: productId, name: "Any"),
        description: nil,
        runtime: IdleChatRuntime(),
        resolveImage: WidgetImageResolver { _ in nil }
    )
}
