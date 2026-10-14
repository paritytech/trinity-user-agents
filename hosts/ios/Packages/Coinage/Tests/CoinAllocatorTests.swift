import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
import Operation_iOS
@testable import Coinage

struct CoinAllocatorTests {
    private let store = StubCurrentInstallationStore(current: .test)
    private let allocator: CoinAllocator

    init() {
        allocator = CoinAllocator(
            installationStore: store,
            coinRepository: AnyDataProviderRepository(StubRepository<Coin>()),
            keyFactory: CoinKeypairFactory(entropyManager: MockEntropyManager(entropy: Data(
                repeating: 0x01,
                count: 32
            )))
        )
    }

    @Test("an allocated coin takes the next item of the current installation")
    func allocateCoin() async throws {
        store.coinItem = 42

        let coin = try await allocator.allocate(exponent: 5, provenance: .unloaded(recyclerFungibility: 73))

        #expect(coin.derivationIndex == CoinageKeyIndex(installation: .test, item: 42))
        #expect(store.coinRequests == 1)
        #expect(store.voucherRequests == 0)
        #expect(coin.exponent == 5)
        #expect(coin.age == nil)
        #expect(coin.recyclerFungibility == 73)
        #expect(coin.hops.isEmpty)
    }

    @Test("an installation with no coins yet starts at item zero and never repeats an item")
    func consecutiveItems() async throws {
        let first = try await allocator.allocate(exponent: 2, provenance: .unknown)
        let second = try await allocator.allocate(exponent: 2, provenance: .unknown)

        #expect(first.derivationIndex == CoinageKeyIndex(installation: .test, item: 0))
        #expect(second.derivationIndex == CoinageKeyIndex(installation: .test, item: 1))
    }

    @Test("persists the provenance it was minted with")
    func allocateCoinWithProvenance() async throws {
        let hops: [Hop] = [.transfer(bundleSize: 3), .split(fanout: 4)]
        let coin = try await allocator.allocate(
            exponent: 2,
            provenance: CoinProvenance(recyclerFungibility: nil, hops: hops)
        )

        #expect(coin.recyclerFungibility == nil)
        #expect(coin.hops == hops)
    }

    @Test("propagates errors from the installation store")
    func storeFailure() async throws {
        store.error = InstallationStubError.unreachable

        await #expect(throws: InstallationStubError.unreachable) {
            try await allocator.allocate(exponent: 0, provenance: .unknown)
        }
    }
}
