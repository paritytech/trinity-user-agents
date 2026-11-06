import struct TrUAPIHost.ProductId
import Foundation
import ChainRegistry
import Individuality
import KeyDerivation
import Products
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("BuiltInPersonhoodKeys")
struct BuiltInPersonhoodKeysTests {
    private let genesisHash = Data(repeating: 0x11, count: 32)
    private let keyManagerFactory = MockBuiltInPersonhoodKeyManagerFactory()
    private let depsResolver = MockMembershipDepsResolver()
    private let chainRegistry = MockChainRegistry()
    private let peopleChainId = "test-people"

    init() {
        let remote = ChainMock.makeRemoteChain(
            id: peopleChainId,
            name: "People",
            genesisHash: genesisHash.toHex(includePrefix: true)
        )
        chainRegistry.chainsById = [peopleChainId: ChainMock.makeChainModel(from: remote, order: 0)]
    }

    @Test("Personhood owner lists the full and lite slots with their People rings")
    func entriesForPersonhoodOwner() async throws {
        let entries = try await makeKeys().entries(owner: try! ProductId("peopl.paseo"))

        #expect(entries == [
            makeEntry(index: 0, collectionId: PeoplePallet.membersIdentifier),
            makeEntry(index: 1, collectionId: PeopleLitePallet.membersIdentifier)
        ])
    }

    @Test("Any other owner has no entries")
    func entriesForOtherOwner() async throws {
        let entries = try await makeKeys().entries(owner: try! ProductId("getcash.paseo"))

        #expect(entries.isEmpty)
    }

    @Test("Listing without Members pallet metadata fails instead of guessing the ring")
    func entriesWithoutPalletIndex() async {
        depsResolver.membersPalletIndex = nil

        await #expect(throws: BuiltInPersonhoodKeyError.peopleChainUnavailable) {
            try await makeKeys().entries(owner: try! ProductId("peopl.paseo"))
        }
    }

    @Test("Unknown People chain fails instead of guessing the ring")
    func entriesWithoutPeopleChain() async {
        chainRegistry.chainsById = [:]

        await #expect(throws: ChainRegistryError.self) {
            try await makeKeys().entries(owner: try! ProductId("peopl.paseo"))
        }
    }

    @Test("Each handle resolves to its own key on its declared ring")
    func resolvesHandleToSlotKey() throws {
        let keys = makeKeys()

        let full = try keys.resolve(handle: handle(0), ring: ring(PeoplePallet.membersIdentifier))
        let lite = try keys.resolve(handle: handle(1), ring: ring(PeopleLitePallet.membersIdentifier))

        #expect(full as? MockBandersnatchKeyManager === keyManagerFactory.fullKeyManager)
        #expect(lite as? MockBandersnatchKeyManager === keyManagerFactory.liteKeyManager)
    }

    @Test("A ring without a pallet junction matches on chain and collection")
    func resolvesWithoutPalletInstance() throws {
        let ring = RingLocation(chainId: genesisHash, junctions: [.collectionId(PeopleLitePallet.membersIdentifier)])

        let key = try makeKeys().resolve(handle: handle(1), ring: ring)

        #expect(key as? MockBandersnatchKeyManager === keyManagerFactory.liteKeyManager)
    }

    @Test(
        "Unknown handles are not registered",
        arguments: [
            RingVrfKeyHandle(productId: try! ProductId("peopl.paseo"), index: .index(2)),
            RingVrfKeyHandle(productId: try! ProductId("peopl.paseo"), index: .raw(Data(repeating: 0, count: 32))),
            RingVrfKeyHandle(productId: try! ProductId("getcash.paseo"), index: .index(1))
        ]
    )
    func unknownHandle(handle: RingVrfKeyHandle) {
        #expect(throws: BuiltInPersonhoodKeyError.keyNotRegistered) {
            try makeKeys().resolve(handle: handle, ring: ring(PeopleLitePallet.membersIdentifier))
        }
    }

    @Test("A handle used on another slot's ring is not in that ring")
    func handleOnWrongRing() {
        #expect(throws: BuiltInPersonhoodKeyError.keyNotInRing) {
            try makeKeys().resolve(handle: handle(0), ring: ring(PeopleLitePallet.membersIdentifier))
        }
    }

    @Test("A handle on another chain is not in the ring")
    func handleOnOtherChain() {
        let ring = RingLocation(
            chainId: Data(repeating: 0x22, count: 32),
            junctions: [.collectionId(PeopleLitePallet.membersIdentifier)]
        )

        #expect(throws: BuiltInPersonhoodKeyError.keyNotInRing) {
            try makeKeys().resolve(handle: handle(1), ring: ring)
        }
    }
}

private extension BuiltInPersonhoodKeysTests {
    func makeKeys() -> BuiltInPersonhoodKeys {
        BuiltInPersonhoodKeys(
            chainRegistry: chainRegistry,
            peopleChainId: peopleChainId,
            depsResolver: depsResolver,
            tldProvider: StubDotNsTldProvider(tld: "paseo"),
            keyManagerFactory: keyManagerFactory
        )
    }

    func handle(_ index: UInt32) -> RingVrfKeyHandle {
        RingVrfKeyHandle(productId: try! ProductId("peopl.paseo"), index: .index(index))
    }

    func ring(_ collectionId: Data) -> RingLocation {
        RingLocation(chainId: genesisHash, junctions: [.palletInstance(50), .collectionId(collectionId)])
    }

    func makeEntry(index: UInt32, collectionId: Data) -> RegisteredRingVrfKey {
        RegisteredRingVrfKey(handle: handle(index), rings: [ring(collectionId)], publicKey: nil)
    }
}
