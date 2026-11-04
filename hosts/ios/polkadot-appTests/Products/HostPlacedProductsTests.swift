import struct TrUAPIHost.ProductId
import Testing
import Keystore_iOS
@testable import polkadot_app

@Suite("Host-placed products")
struct HostPlacedProductsTests {
    /// The first stage of individuality-apps#1281: Humanity's `polkadot` room is placed beside the
    /// game's `jollity`, which stays placed so the game's worker can hand over before a later
    /// release stops placing it. The room ids are the contract with each product's worker.
    @Test("Humanity's Polkadot room is placed beside the game's Jollity room")
    func humanityIsPlacedBesideTheGame() throws {
        let placed = try HostPlacedProducts.all.map { try ($0.productId("paseo").rawValue, $0.roomId, $0.fallbackName) }

        #expect(placed.count == 2)
        #expect(placed.contains { $0 == ("peopl.paseo", "polkadot", "Polkadot") })
        #expect(placed.contains { $0 == ("dim2.paseo", "jollity", "Jollity") })
    }

    /// A reserved identity lives in each network's own namespace, so one entry has to answer for
    /// whichever chain the build is pointed at. Writing the product id out would silently stop
    /// placing it on Paseo.
    @Test(
        "A placed room matches across TLDs",
        arguments: [
            ("dim2.dot", "jollity"), ("dim2.paseo", "jollity"), ("dim2.testnet", "jollity"),
            ("peopl.dot", "polkadot"), ("peopl.paseo", "polkadot"), ("peopl.testnet", "polkadot")
        ]
    )
    func placedRoomMatchesEveryTld(input: String, roomId: String) throws {
        #expect(try HostPlacedProducts.placesRoom(roomId, of: ProductId(input)))
    }

    /// Only the room the host placed: Humanity's earlier `humanity` room, a room named after the
    /// other placed product, and a product's roomless chat are not placed.
    @Test(
        "Only the placed room of a placed product is placed",
        arguments: [
            ("peopl.paseo", "humanity"), ("peopl.paseo", "jollity"), ("dim2.paseo", "polkadot"), ("dim2.dot", "game")
        ]
    )
    func otherRoomsOfAPlacedProductAreNotPlaced(input: String, roomId: String) throws {
        #expect(try !HostPlacedProducts.placesRoom(roomId, of: ProductId(input)))
        #expect(try !HostPlacedProducts.placesRoom(nil, of: ProductId(input)))
    }

    /// An exact match, so a subdomain is a separate product and a separate publisher, and a bare
    /// label with no TLD is not a product id at all. Same rule the core applies to these products
    /// in `truapi::platform::has_trusted_remote_permissions`.
    @Test(
        "Only the reserved identity itself is host-placed",
        arguments: [
            "app.dim2.dot", "dim2x.dot", "xdim2.dot", "notdim2.paseo", "app.peopl.paseo", "peoplx.dot",
            "localhost", "localhost:3000"
        ]
    )
    func neighbouringIdentifiersAreNotHostPlaced(input: String) throws {
        let productId = try ProductId(input)

        #expect(!HostPlacedProducts.placesRoom("jollity", of: productId))
        #expect(!HostPlacedProducts.placesRoom("polkadot", of: productId))
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

@Suite("Pinning a host-placed room to the top of the chat list")
struct HostPlacedChatPinningTests {
    /// The host placed the room, so it holds the top of the list rather than competing for it on
    /// recency. Without this the placed room sinks under any chat that got a message later, which
    /// on a fresh install is every chat.
    @Test("Every host-placed room is pinned")
    func hostPlacedRoomIsPinned() throws {
        for hostPlaced in HostPlacedProducts.all {
            let peer = try Chat.Peer.chatExtension(hostPlaced.productId("paseo").rawValue, roomId: hostPlaced.roomId)

            #expect(peer.isPinnedToTop, "\(hostPlaced.roomId)")
        }
    }

    /// Placing Humanity lists every room it has, including the `humanity` room its worker
    /// registered before; that one must not sit pinned and empty beside Polkadot.
    @Test("Another room of a placed product is not pinned")
    func anotherRoomOfAPlacedProductIsNotPinned() {
        #expect(!Chat.Peer.chatExtension("peopl.paseo", roomId: "humanity").isPinnedToTop)
        #expect(!Chat.Peer.chatExtension("peopl.paseo", roomId: nil).isPinnedToTop)
    }

    @Test("An ordinary product's room is not pinned")
    func ordinaryProductIsNotPinned() {
        #expect(!Chat.Peer.chatExtension("coinflip.paseo", roomId: nil).isPinnedToTop)
        #expect(!Chat.Peer.chatExtension("coinflip.paseo", roomId: "polkadot").isPinnedToTop)
    }
}
