import Foundation

@testable import Coinage

/// Mock coin allocator that returns coins with sequential derivation indices and records every
/// coin it mints, so tests can assert on the outputs the strategies persist.
actor MockCoinAllocator: CoinAllocating, CoinMinting {
    private var nextIndex: CoinageKeyIndex = 100
    private(set) var mintedCoins: [Coin] = []

    func allocate(exponent: Int16, provenance: CoinProvenance) async throws -> Coin {
        let index = nextIndex
        nextIndex = nextIndex.next()
        let coin = Coin(
            exponent: exponent,
            derivationIndex: index,
            age: nil,
            recyclerFungibility: provenance.recyclerFungibility,
            hops: provenance.hops,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
        )
        mintedCoins.append(coin)
        return coin
    }

    func mintCoin(exponent: Int16, provenance: CoinProvenance) async throws -> Coin {
        try await allocate(exponent: exponent, provenance: provenance)
    }
}
