import Testing
import Keystore_iOS
import KeyDerivation
import Products
@testable import polkadot_app

@Suite("Host-placed products")
struct HostPlacedProductsTests {
    /// The Jollity leg of #562: a Release build has to carry the game's chat with no add, install
    /// or discover step. The list is what the host places; whether it places at all is the setting.
    @Test("Jollity is host-placed on every runtime")
    func jollityIsTheHostPlacedProduct() throws {
        let hostPlaced = try #require(HostPlacedProducts.all.first)

        #expect(hostPlaced.fallbackName == "Jollity")
        #expect(hostPlaced.productId("paseo") == BuiltInProduct.dim2(for: "paseo"))
    }

    /// The chat product runs the person chats in every build, on the core's `PersonChat`. Its room id
    /// is the home room the product registers, and it places no room of its own.
    @Test("The bundled chat product is host-placed in every build")
    func chatIsHostPlacedInEveryBuild() throws {
        let chat = try #require(HostPlacedProducts.all.first { $0.productId("paseo") == "chat.paseo" })

        #expect(HostPlacedProducts.all.count == 2)
        #expect(chat.roomId == "chats")
        #expect(!chat.placesRoom)
        #expect(HostPlacedProducts.contains(productId: "chat.dot"))
        #expect(!HostPlacedProducts.contains(productId: "app.chat.dot"))
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

@Suite("The bundled chat product")
struct BundledChatProductResolverTests {
    /// The chat label reaches `PersonChat`, so whatever dotNS publishes under it must not run before verified updates
    /// exist: the resolver gives no published worker, and the app serves the bundle it installed.
    @Test("chat.<tld> never resolves to a published worker")
    func chatNeverResolvesToAPublishedWorker() async throws {
        let inner = CountingResolver()
        let resolver = BundledChatProductResolver(resolver: inner)

        let chat = try await resolver.resolve("chat.paseo")
        _ = try await resolver.resolve("coinflip.paseo")

        #expect(chat.executables.worker == nil)
        #expect(!chat.hasManifest)
        #expect(await inner.resolved == ["coinflip.paseo"])
        #expect(!BundledChatProductResolver.isChatProduct("app.chat.paseo"))
    }
}

private actor CountingResolver: ProductResolving {
    private(set) var resolved: [String] = []

    func resolve(_ productId: ProductId) async throws -> ResolvedProduct {
        resolved.append(productId)
        return .legacy(id: productId)
    }
}
