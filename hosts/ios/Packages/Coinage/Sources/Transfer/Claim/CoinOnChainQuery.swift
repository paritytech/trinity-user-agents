import AsyncExtensions
import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateStorageSubscription
import SubstrateSdkExt
import Individuality

enum CoinOnChainQueryError: Error {
    case timeout
    case subscriptionTerminated
}

/// What the chain shows for the coins a claim waits on: the ones of the app's Coinage instance it can
/// claim, and the foreign coins it must refuse, with their Coinage instance.
struct CoinInfosSnapshot: Equatable {
    var claimable: [Data: ClaimableCoinInfo] = [:]
    var foreign: [Data: CoinageInstanceId] = [:]
}

/// Batch-fetches on-chain coin state for given public keys.
protocol CoinOnChainQuerying: Sendable {
    /// Reads `CoinsByOwner` for multiple public keys at a specific block hash, judged against the app's
    /// Coinage instance. Returns one read per key, in input order.
    func fetchCoinReads(for publicKeys: [Data], atBlockHash: Data?) async throws -> [OnChainCoinRead]

    /// Waits until all coins for the given public keys appear on-chain, or throws when the subscription terminates.
    func awaitAllCoinsOnChain(for publicKeys: [Data]) async throws

    /// Waits until all coins for the given public keys are absent/spent on-chain, or throws when the subscription
    /// terminates.
    func awaitAllCoinsOffChain(for publicKeys: [Data]) async throws

    /// Waits until every key has been observed on-chain at least once during the subscription
    /// (present now, or present earlier then spent — i.e. claimed). Returns `true` if at least one
    /// key is still present at resolution (awaiting claim), `false` if all observed keys are now
    /// spent (already claimed). Throws when the subscription terminates first. Callers race against
    /// a block timeout.
    func awaitAllCoinsSentOrClaimed(for publicKeys: [Data]) async throws -> Bool

    /// Subscribes to on-chain coin state for `publicKeys`, yielding on every block where any of them
    /// change: the claimable coins with their value exponent, and the foreign coins. A key that goes absent
    /// (spent) drops out; a never-landed coin never appears. Callers bound the wait themselves.
    func subscribeCoinInfos(for publicKeys: [Data]) -> AnyAsyncSequence<CoinInfosSnapshot>
}

extension CoinOnChainQuerying {
    /// Fetches on-chain coins of the app's Coinage instance at a specific block hash; a foreign coin
    /// reads as `nil`. Returns an array of optionals in the same order as the input keys.
    func fetchCoins(for publicKeys: [Data], atBlockHash: Data?) async throws -> [CoinSyncResult.OnChainCoin?] {
        try await fetchCoinReads(for: publicKeys, atBlockHash: atBlockHash).map(\.coin)
    }

    /// Fetches on-chain coins for multiple public keys in a single RPC call.
    /// Returns an array of optionals in the same order as the input keys.
    func fetchCoins(for publicKeys: [Data]) async throws -> [CoinSyncResult.OnChainCoin?] {
        try await fetchCoins(for: publicKeys, atBlockHash: nil)
    }

    /// Default: presence is the only confirmation (legacy semantics). Conformers that can observe
    /// the spent transition override this to also treat a claimed coin as proof of send.
    func awaitAllCoinsSentOrClaimed(for publicKeys: [Data]) async throws -> Bool {
        try await awaitAllCoinsOnChain(for: publicKeys)
        return true
    }
}

/// Default implementation that queries the Coinage pallet storage via RPC.
final class CoinOnChainQueryService: CoinOnChainQuerying, @unchecked Sendable {
    private let instanceId: CoinageInstanceId
    private let connection: any JSONRPCEngine
    private let runtimeService: any RuntimeCodingServiceProtocol
    private let storageRequestFactory: any StorageRequestFactoryProtocol

    init(
        instanceId: CoinageInstanceId,
        connection: any JSONRPCEngine,
        runtimeService: any RuntimeCodingServiceProtocol,
        storageRequestFactory: any StorageRequestFactoryProtocol
    ) {
        self.instanceId = instanceId
        self.connection = connection
        self.runtimeService = runtimeService
        self.storageRequestFactory = storageRequestFactory
    }

    func fetchCoinReads(for publicKeys: [Data], atBlockHash: Data?) async throws -> [OnChainCoinRead] {
        guard !publicKeys.isEmpty else { return [] }

        let coderFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()
        let coinPath = CoinagePallet.Storage.coinsByOwner

        let queryWrapper: CompoundOperationWrapper<[StorageResponse<CoinSyncResult.OnChainCoin>]> =
            storageRequestFactory.queryItems(
                engine: connection,
                keyParams: { publicKeys.map { BytesCodable(wrappedValue: $0) } },
                factory: { coderFactory },
                storagePath: coinPath(),
                options: StorageQueryListOptions(atBlock: atBlockHash)
            )

        let responses = try await queryWrapper.asyncExecute()
        return responses.map { $0.value.read(in: instanceId) }
    }

    /// Subscribes to on-chain coin storage and resolves when all requested keys have a non-nil coin.
    ///
    /// Each emission may be a partial update (only keys changed in that block), so state is
    /// accumulated across emissions. Throws `subscriptionTerminated` if the stream ends before
    /// all keys are satisfied. Callers are responsible for racing this against a block-based timeout.
    func awaitAllCoinsOnChain(for publicKeys: [Data]) async throws {
        guard !publicKeys.isEmpty else { return }

        let mappingKeys = Set(publicKeys.map { $0.toHex() })

        let accumulatedStream = subscribeCoins(for: publicKeys)
            .scan([String: CoinSyncResult.OnChainCoin]()) { accumulated, updates in
                var next = accumulated
                for (key, coin) in updates where coin != nil {
                    next[key] = coin
                }
                return next
            }

        for try await accumulated in accumulatedStream {
            guard mappingKeys.allSatisfy({ accumulated[$0] != nil }) else {
                continue
            }
            return
        }

        // Stream ended without satisfying all keys (e.g. websocket disconnect)
        throw CoinOnChainQueryError.subscriptionTerminated
    }

    /// Subscribes to on-chain coin storage and resolves when all requested keys have a nil coin (spent/absent).
    ///
    /// Accumulates state across partial emissions. Throws `subscriptionTerminated` if the stream
    /// ends before all keys are spent. Callers are responsible for racing against a block timeout.
    func awaitAllCoinsOffChain(for publicKeys: [Data]) async throws {
        guard !publicKeys.isEmpty else { return }

        let mappingKeys = Set(publicKeys.map { $0.toHex() })

        let accumulatedStream = subscribeCoins(for: publicKeys)
            .scan([String: CoinSyncResult.OnChainCoin]()) { accumulated, updates in
                var next = accumulated
                for (key, coin) in updates {
                    if coin != nil {
                        next[key] = coin
                    } else {
                        next.removeValue(forKey: key)
                    }
                }
                return next
            }

        for try await accumulated in accumulatedStream {
            guard mappingKeys.allSatisfy({ accumulated[$0] == nil }) else {
                continue
            }
            return
        }

        throw CoinOnChainQueryError.subscriptionTerminated
    }

    /// Subscribes to on-chain coin storage and resolves once every key has been seen present at
    /// least once (present now, or present then spent). A never-landed coin is never observed, so
    /// the caller's block timeout must bound the wait. Returns whether any key is still present.
    func awaitAllCoinsSentOrClaimed(for publicKeys: [Data]) async throws -> Bool {
        guard !publicKeys.isEmpty else { return true }

        let mappingKeys = Set(publicKeys.map { $0.toHex() })

        let accumulatedStream = subscribeCoins(for: publicKeys)
            .scan((seen: Set<String>(), present: Set<String>())) { state, updates in
                var seen = state.seen
                var present = state.present
                for (key, coin) in updates {
                    if coin != nil {
                        seen.insert(key)
                        present.insert(key)
                    } else {
                        present.remove(key)
                    }
                }
                return (seen: seen, present: present)
            }

        for try await state in accumulatedStream {
            guard mappingKeys.allSatisfy({ state.seen.contains($0) }) else {
                continue
            }
            return !state.present.isEmpty
        }

        throw CoinOnChainQueryError.subscriptionTerminated
    }

    func subscribeCoinInfos(for publicKeys: [Data]) -> AnyAsyncSequence<CoinInfosSnapshot> {
        guard !publicKeys.isEmpty else {
            return AsyncStream<CoinInfosSnapshot> { $0.finish() }.eraseToAnyAsyncSequence()
        }

        let dataByHex = Dictionary(publicKeys.map { ($0.toHex(), $0) }, uniquingKeysWith: { first, _ in first })

        return subscribeCoinReads(for: publicKeys)
            .scan([String: OnChainCoinRead]()) { accumulated, reads in
                accumulated.merging(reads) { _, latest in latest }
            }
            .map { readsByHex -> CoinInfosSnapshot in
                var snapshot = CoinInfosSnapshot()
                for (hex, read) in readsByHex {
                    guard let key = dataByHex[hex] else { continue }
                    switch read {
                    case let .present(coin):
                        snapshot.claimable[key] = ClaimableCoinInfo(exponent: Int16(coin.value), age: coin.age)
                    case let .foreign(instanceId):
                        snapshot.foreign[key] = instanceId
                    case .absent:
                        break
                    }
                }
                return snapshot
            }
            .eraseToAnyAsyncSequence()
    }

    /// `subscribeCoinReads` with a foreign coin read as absent.
    private func subscribeCoins(for publicKeys: [Data]) -> AnyAsyncSequence<[String: CoinSyncResult.OnChainCoin?]> {
        subscribeCoinReads(for: publicKeys)
            .map { $0.mapValues(\.coin) }
            .eraseToAnyAsyncSequence()
    }

    /// Opens a Substrate storage subscription for `CoinsByOwner` entries keyed by public key.
    ///
    /// Uses the same `MapSubscriptionRequest` + `CallbackBatchStorageSubscription` pattern
    /// as `CoinStateSyncService.performSync()`. The returned stream emits each key's read against the
    /// app's Coinage instance on every block where any of the watched storage keys change.
    private func subscribeCoinReads(for publicKeys: [Data]) -> AnyAsyncSequence<[String: OnChainCoinRead]> {
        let requests: [BatchStorageSubscriptionRequest] = publicKeys.map { publicKey in
            let mappingKey = publicKey.toHex()
            let storagePath = CoinagePallet.Storage.coinsByOwner
            let innerRequest = MapSubscriptionRequest(
                storagePath: storagePath(),
                localKey: "",
                keyParamClosure: { BytesCodable(wrappedValue: publicKey) }
            )
            return BatchStorageSubscriptionRequest(innerRequest: innerRequest, mappingKey: mappingKey)
        }

        let results: AnyAsyncSequence<CoinSyncResult> = CallbackBatchStorageSubscription.asyncStream(
            requests: requests,
            connection: connection,
            runtimeService: runtimeService,
            logger: nil
        )

        return results
            .map { [instanceId] in $0.reads(in: instanceId) }
            .eraseToAnyAsyncSequence()
    }
}
