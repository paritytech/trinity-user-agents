import AsyncExtensions
import Foundation
import os
@testable import Coinage

/// Answers from a per-block presence table (`nil` block = best head) and records every read.
final class StubCoinOnChainQuery: CoinOnChainQuerying, @unchecked Sendable {
    struct Read: Equatable {
        let keys: [PublicKey]
        let atBlockHash: Data?
    }

    private let state = OSAllocatedUnfairLock<(
        presence: [Data?: [PublicKey: CoinSyncResult.OnChainCoin]],
        reads: [Read]
    )>(initialState: ([:], []))

    var reads: [Read] { state.withLock { $0.reads } }

    func setPresent(_ key: PublicKey, atBlockHash blockHash: Data?, value: Int8 = 1, age: Int16 = 0) {
        state.withLock {
            $0.presence[blockHash, default: [:]][key] = CoinSyncResult.OnChainCoin(
                instanceId: 0,
                value: value,
                age: age
            )
        }
    }

    func setAbsent(_ key: PublicKey, atBlockHash blockHash: Data?) {
        state.withLock { $0.presence[blockHash]?[key] = nil }
    }

    func fetchCoins(for publicKeys: [Data], atBlockHash: Data?) async throws -> [CoinSyncResult.OnChainCoin?] {
        state.withLock { state in
            state.reads.append(Read(keys: publicKeys, atBlockHash: atBlockHash))
            let table = state.presence[atBlockHash] ?? [:]
            return publicKeys.map { table[$0] }
        }
    }

    func awaitAllCoinsOnChain(for _: [Data]) async throws {
        throw StubCoinOnChainQueryError.unsupported("awaitAllCoinsOnChain")
    }

    func awaitAllCoinsOffChain(for _: [Data]) async throws {
        throw StubCoinOnChainQueryError.unsupported("awaitAllCoinsOffChain")
    }

    /// One snapshot of the keys present at the best head, then finishes.
    func subscribeCoinInfos(for publicKeys: [Data]) -> AnyAsyncSequence<[Data: ClaimableCoinInfo]> {
        let present = state.withLock { $0.presence[nil] ?? [:] }
        let infos = publicKeys.reduce(into: [Data: ClaimableCoinInfo]()) { result, key in
            guard let coin = present[key] else { return }
            result[key] = ClaimableCoinInfo(exponent: Int16(coin.value), age: coin.age)
        }

        return AsyncStream<[Data: ClaimableCoinInfo]> { continuation in
            continuation.yield(infos)
            continuation.finish()
        }
        .eraseToAnyAsyncSequence()
    }
}

enum StubCoinOnChainQueryError: Error {
    case unsupported(String)
}
