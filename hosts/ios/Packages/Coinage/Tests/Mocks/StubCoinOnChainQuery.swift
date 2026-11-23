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

    /// The Coinage instance the stub reads coins against; a coin of any other instance is a foreign coin.
    static let appInstance: CoinageInstanceId = 0

    var reads: [Read] { state.withLock { $0.reads } }

    func setPresent(_ key: PublicKey, atBlockHash blockHash: Data?, value: Int8 = 1, age: Int16 = 0) {
        state.withLock {
            $0.presence[blockHash, default: [:]][key] = CoinSyncResult.OnChainCoin(
                instanceId: Self.appInstance,
                value: value,
                age: age
            )
        }
    }

    func setForeign(_ key: PublicKey, atBlockHash blockHash: Data?, instanceId: CoinageInstanceId) {
        state.withLock {
            $0.presence[blockHash, default: [:]][key] = CoinSyncResult.OnChainCoin(
                instanceId: instanceId,
                value: 1,
                age: 0
            )
        }
    }

    func setAbsent(_ key: PublicKey, atBlockHash blockHash: Data?) {
        state.withLock { $0.presence[blockHash]?[key] = nil }
    }

    func fetchCoinReads(for publicKeys: [Data], atBlockHash: Data?) async throws -> [OnChainCoinRead] {
        state.withLock { state in
            state.reads.append(Read(keys: publicKeys, atBlockHash: atBlockHash))
            let table = state.presence[atBlockHash] ?? [:]
            return publicKeys.map { table[$0].read(in: Self.appInstance) }
        }
    }

    func awaitAllCoinsOnChain(for _: [Data]) async throws {
        throw StubCoinOnChainQueryError.unsupported("awaitAllCoinsOnChain")
    }

    func awaitAllCoinsOffChain(for _: [Data]) async throws {
        throw StubCoinOnChainQueryError.unsupported("awaitAllCoinsOffChain")
    }

    /// One snapshot of the keys present at the best head, then finishes.
    func subscribeCoinInfos(for publicKeys: [Data]) -> AnyAsyncSequence<CoinInfosSnapshot> {
        let present = state.withLock { $0.presence[nil] ?? [:] }
        let infos = publicKeys.reduce(into: [Data: ClaimableCoinInfo]()) { result, key in
            guard let coin = present[key].read(in: Self.appInstance).coin else { return }
            result[key] = ClaimableCoinInfo(exponent: Int16(coin.value), age: coin.age)
        }

        return AsyncStream<CoinInfosSnapshot> { continuation in
            continuation.yield(CoinInfosSnapshot(claimable: infos))
            continuation.finish()
        }
        .eraseToAnyAsyncSequence()
    }
}

enum StubCoinOnChainQueryError: Error {
    case unsupported(String)
}
