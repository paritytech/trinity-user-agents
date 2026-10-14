import Foundation
import os
@testable import Coinage

/// Serves a configurable tracked-coin set; `save` is a no-op.
final class StubCoinService: CoinServiceProtocol, @unchecked Sendable {
    /// The store is simply unavailable — transport down, context gone. It knows nothing about any
    /// particular coin, which is why callers must not read its failure as a verdict on one.
    enum Failure: Error { case unavailable }

    private let coins = OSAllocatedUnfairLock(initialState: [TrackedCoin]())
    private let failure = OSAllocatedUnfairLock(initialState: (any Error)?.none)

    init(coins: [TrackedCoin] = [], fetchError: (any Error)? = nil) {
        set(coins: coins)
        set(fetchError: fetchError)
    }

    func set(coins: [TrackedCoin]) {
        self.coins.withLock { $0 = coins }
    }

    /// Makes every read fail. Set rather than subclassed so the reads a caller makes against an
    /// unreadable store are the same reads it makes against a readable one.
    func set(fetchError: (any Error)?) {
        failure.withLock { $0 = fetchError }
    }

    func fetchAllTrackedCoins() async throws -> [TrackedCoin] {
        try throwIfFailing()
        return coins.withLock { $0 }
    }

    func fetchCoins(publicKeys: Set<PublicKey>) async throws -> Set<Coin> {
        try throwIfFailing()
        return Set(coins.withLock { $0 }.map(\.coin).filter { publicKeys.contains($0.publicKey) })
    }

    func save(coins _: [Coin]) async throws {
        try throwIfFailing()
    }

    private func throwIfFailing() throws {
        if let error = failure.withLock({ $0 }) {
            throw error
        }
    }
}
