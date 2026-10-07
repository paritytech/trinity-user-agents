import Testing
import Keystore_iOS
import KeyDerivation
@testable import polkadot_app

@Suite("Host-placed products")
struct HostPlacedProductsTests {
    /// The list is what the host places; whether it places at all is the setting. Jollity is the
    /// leg of #562: a Release build has to carry the game's chat with no add, install or discover
    /// step. Humanity's worker has to run before anyone opens the SPA, or the chat it posts into
    /// never appears.
    @Test("Jollity and Humanity are the host-placed products")
    func jollityAndHumanityAreTheHostPlacedProducts() {
        #expect(HostPlacedProducts.all.map(\.roomId) == ["jollity", "humanity"])
        #expect(HostPlacedProducts.all.map(\.fallbackName) == ["Jollity", "Humanity"])
        #expect(
            HostPlacedProducts.all.map { $0.productId("paseo") }
                == [BuiltInProduct.dim2(for: "paseo"), BuiltInProduct.personhood(for: "paseo")]
        )
    }

    /// A reserved identity lives in each network's own namespace, so one entry has to answer for
    /// whichever chain the build is pointed at. Writing the product id out would silently stop
    /// placing it on Paseo.
    @Test(
        "A reserved identity matches across TLDs",
        arguments: ["dim2.dot", "dim2.paseo", "dim2.testnet", "peopl.dot", "peopl.paseo", "peopl.testnet"]
    )
    func reservedIdentityMatchesEveryTld(productId: String) {
        #expect(HostPlacedProducts.contains(productId: productId))
    }

    /// An exact match, so a subdomain is a separate product and a separate publisher, and a bare
    /// label with no TLD is not a product id at all. Same rule the core applies to these products
    /// in `truapi::platform::has_trusted_remote_permissions`.
    @Test(
        "Only the reserved identity itself is host-placed",
        arguments: [
            "app.dim2.dot", "dim2x.dot", "xdim2.dot", "dim2", "", "notdim2.paseo",
            "app.peopl.paseo", "people.paseo", "peopl"
        ]
    )
    func neighbouringIdentifiersAreNotHostPlaced(productId: String) {
        #expect(!HostPlacedProducts.contains(productId: productId))
    }
}

@Suite("The host-placement switch")
struct HostPlacementSettingTests {
    /// Only Jollity has a native twin (WeeklyGame) to keep apart from, so only Jollity waits on the
    /// switch; Humanity's worker has to boot on Nightly too.
    @Test("Only Jollity follows the placement switch")
    func onlyJollityFollowsTheSwitch() {
        #expect(HostPlacedProducts.placed(enabled: false).map(\.roomId) == ["humanity"])
        #expect(HostPlacedProducts.placed(enabled: true).map(\.roomId) == ["jollity", "humanity"])
    }

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
    @Test("Every host-placed product's bot is pinned", arguments: HostPlacedProducts.all.map(\.roomId))
    func hostPlacedBotIsPinned(roomId: String) throws {
        let hostPlaced = try #require(HostPlacedProducts.all.first { $0.roomId == roomId })
        let peer = Chat.Peer.chatExtension(hostPlaced.productId("paseo"), roomId: nil)

        #expect(peer.isPinnedToTop)
    }

    /// The native extension keeps its pin on the builds that still have it. It is a chat identity
    /// of its own, not the product's, which is why the two can coexist and why only the build
    /// default keeps them apart.
    @Test("The native DIM2 extension stays pinned")
    func nativeExtensionStaysPinned() {
        #expect(Chat.Peer.chatExtension(DIM2ChatExtension.identifier, roomId: nil).isPinnedToTop)
        #expect(!HostPlacedProducts.contains(productId: DIM2ChatExtension.identifier))
    }

    @Test("An ordinary product's bot is not pinned")
    func ordinaryProductIsNotPinned() {
        #expect(!Chat.Peer.chatExtension("coinflip.paseo", roomId: nil).isPinnedToTop)
    }
}

@Suite("A product's first registration adopts the placed room")
struct HostPlacedPlaceholderTests {
    /// The placer's row has no icon and no message; `createRoom` adopts it and answers `new`, so
    /// the product's first-boot welcome still runs and its icon lands.
    @Test("The placer's row is a placeholder")
    func placedRowIsPlaceholder() {
        let placed = Chat.LocalModel.newChatWithRoom(
            extensionId: "peopl.paseo",
            roomId: "humanity",
            roomMetadata: Chat.RoomMetadata(chatRelativeId: "humanity", name: "Humanity", icon: nil)
        )

        #expect(placed.isHostPlacedPlaceholder)
    }

    @Test("A room the product registered with an icon is not")
    func registeredRoomIsNotPlaceholder() {
        let registered = Chat.LocalModel.newChatWithRoom(
            extensionId: "peopl.paseo",
            roomId: "humanity",
            roomMetadata: Chat.RoomMetadata(chatRelativeId: "humanity", name: "Humanity", icon: "data:image/png;base64,AA==")
        )

        #expect(!registered.isHostPlacedPlaceholder)
    }
}
