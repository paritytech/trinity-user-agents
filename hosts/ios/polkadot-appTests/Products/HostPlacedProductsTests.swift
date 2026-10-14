import Testing
import Keystore_iOS
import KeyDerivation
@testable import polkadot_app

@Suite("Host-placed products")
struct HostPlacedProductsTests {
    /// The Jollity leg of #562: a Release build has to carry the game's chat with no add, install
    /// or discover step. The list is what the host places; whether it places at all is the setting.
    @Test("Jollity is the host-placed product")
    func jollityIsTheHostPlacedProduct() throws {
        let hostPlaced = try #require(HostPlacedProducts.all.first)

        #expect(HostPlacedProducts.all.count == 1)
        #expect(hostPlaced.fallbackName == "Jollity")
        #expect(hostPlaced.productId("paseo") == BuiltInProduct.dim2(for: "paseo"))
    }

    /// A reserved identity lives in each network's own namespace, so one entry has to answer for
    /// whichever chain the build is pointed at. Writing the product id out would silently stop
    /// placing it on Paseo.
    @Test("A reserved identity matches across TLDs", arguments: ["dim2.dot", "dim2.paseo", "dim2.test"])
    func reservedIdentityMatchesEveryTld(productId: String) {
        #expect(HostPlacedProducts.contains(productId: productId))
    }

    /// An exact match, so a subdomain is a separate product and a separate publisher, and a bare
    /// label with no TLD is not a product id at all. Same rule the core applies to these products
    /// in `truapi::platform::has_trusted_remote_permissions`.
    @Test(
        "Only the reserved identity itself is host-placed",
        arguments: ["app.dim2.dot", "dim2x.dot", "xdim2.dot", "dim2", "", "notdim2.paseo"]
    )
    func neighbouringIdentifiersAreNotHostPlaced(productId: String) {
        #expect(!HostPlacedProducts.contains(productId: productId))
    }
}

@Suite("The host-placement switch")
struct HostPlacementSettingTests {
    /// The override exists so a tester can see either arrangement, including both bots at once,
    /// without building a second configuration.
    @Test("An explicit switch off is respected")
    func storedOffChoiceWins() {
        let settings = InMemorySettingsManager()
        settings.set(value: false, for: .hostPlacementEnabled)

        #expect(!settings.isHostPlacementEnabled)
    }

    @Test("An explicit switch on is respected")
    func storedOnChoiceWins() {
        let settings = InMemorySettingsManager()
        settings.set(value: true, for: .hostPlacementEnabled)

        #expect(settings.isHostPlacementEnabled)
    }
}

@Suite("Pinning a host-placed bot to the top of the chat list")
struct HostPlacedChatPinningTests {
    /// The host placed the bot, so it holds the top of the list rather than competing for it on
    /// recency. Without this the placed room sinks under any chat that got a message later, which
    /// on a fresh install is every chat.
    @Test("A host-placed product's bot is pinned")
    func hostPlacedBotIsPinned() throws {
        let hostPlaced = try #require(HostPlacedProducts.all.first)
        let peer = Chat.Peer.chatExtension(hostPlaced.productId("paseo"), roomId: nil)

        #expect(peer.isPinnedToTop)
    }

    @Test("An ordinary product's bot is not pinned")
    func ordinaryProductIsNotPinned() {
        #expect(!Chat.Peer.chatExtension("coinflip.paseo", roomId: nil).isPinnedToTop)
    }
}
