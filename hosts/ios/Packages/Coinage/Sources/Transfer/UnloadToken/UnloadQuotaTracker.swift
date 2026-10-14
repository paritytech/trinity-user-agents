import Foundation
import FoundationExt
import SubstrateSdk
import Individuality
import SubstrateOperation

/// Remaining free-unload tokens this period, together with the period allowance so callers can
/// express a reserve as a fraction of the limit rather than a fixed count.
public struct UnloadQuota: Equatable, Sendable {
    public let remaining: Int
    public let limit: Int

    public init(remaining: Int, limit: Int) {
        self.remaining = remaining
        self.limit = limit
    }
}

/// Reads how much free-unload quota is left, so the recycling quota valve can auto-manage privacy
/// once the allowance runs low. Quota is consumed at unload time and scales with the number of unload
/// batches, so a high-privacy strategy raises pressure here.
public protocol UnloadQuotaTracking: Sendable {
    func remainingQuota() async throws -> UnloadQuota
    /// Decrements the cached estimate by `count` after that many successful unloads rather than
    /// re-walking the range. Called by the unload paths (transfers / external payments) — the only
    /// operations that spend free-unload tokens. Recycling loads coins under a coin origin and spends
    /// none, so it must not call this.
    func noteUnloadHappened(count: Int) async
}

/// Counts unconsumed counters across the valid periods, caching the result for the current period.
/// The count is exact while tokens are consumed in index order (which `UnloadTokenResolver` does): the
/// consumed counters form a prefix, so the walk stops at the first free one and treats the rest as free.
public actor UnloadQuotaTracker: UnloadQuotaTracking {
    private let runtimeCodingService: RuntimeCodingServiceProtocol
    private let consumedTokenChecker: any ConsumedTokenChecking
    private let personOriginProvider: any OriginPersonProviding
    private let viewFunctionFetcher: any ViewFunctionFetching
    private let dateProvider: any DateProviding

    private struct Cache {
        let period: UInt32
        var quota: UnloadQuota
    }

    private var cache: Cache?
    private var unloadsSinceWalk = 0

    /// A full re-walk every this many unloads bounds incremental drift from the decrement path.
    private static let unloadsBeforeRefresh = 5

    /// Counters queried per chain call; a batch that ends on a free counter stops the walk.
    private static let batchSize: UInt32 = 100

    public init(
        runtimeCodingService: RuntimeCodingServiceProtocol,
        consumedTokenChecker: any ConsumedTokenChecking,
        personOriginProvider: any OriginPersonProviding,
        viewFunctionFetcher: any ViewFunctionFetching,
        dateProvider: any DateProviding
    ) {
        self.runtimeCodingService = runtimeCodingService
        self.consumedTokenChecker = consumedTokenChecker
        self.personOriginProvider = personOriginProvider
        self.viewFunctionFetcher = viewFunctionFetcher
        self.dateProvider = dateProvider
    }

    public func remainingQuota() async throws -> UnloadQuota {
        let periodDuration: UInt64 = try await runtimeCodingService.fetchConstant(
            path: CoinagePallet.Constants.unloadTokenTimePeriod(),
            type: UInt64.self
        )

        let wrappedMaxCounter: StringCodable<UInt32> = try await viewFunctionFetcher.fetch(
            viewFunction: CoinagePallet.ViewFunction.maxFreeUnloadTokensPerTimePeriod()
        )

        let maxCounter = wrappedMaxCounter.wrappedValue

        let periods = await UnloadTokenPeriodCalculator.validPeriods(
            currentDate: dateProvider.read(),
            periodDuration: periodDuration
        )
        let currentPeriod = periods.last ?? 0

        // Cache keyed by current period, so a period boundary invalidates it for free.
        if let cache, cache.period == currentPeriod {
            return cache.quota
        }

        let quota = try await walk(periods: periods, maxCounter: maxCounter)
        cache = Cache(period: currentPeriod, quota: quota)
        unloadsSinceWalk = 0
        return quota
    }

    public func noteUnloadHappened(count: Int) {
        guard count > 0 else { return }
        unloadsSinceWalk += count

        if unloadsSinceWalk >= Self.unloadsBeforeRefresh {
            cache = nil
            unloadsSinceWalk = 0
        } else if let current = cache {
            let decremented = UnloadQuota(
                remaining: max(0, current.quota.remaining - count),
                limit: current.quota.limit
            )
            cache = Cache(period: current.period, quota: decremented)
        }
    }
}

extension UnloadQuotaTracker {
    /// Counts the free counters in `0 ..< maxCounter`, querying a batch at a time and stopping as soon as
    /// a batch ends on a free counter: because tokens are taken in index order the consumed counters are a
    /// prefix, so everything past a free one is free too and the tail need not be queried. `consumedInBatch`
    /// returns the consumed flags (`true` = consumed) for a half-open counter range. Pure and side-effect
    /// free so the paging can be unit-tested without the chain.
    static func countFreeCounters(
        maxCounter: UInt32,
        batchSize: UInt32,
        consumedInBatch: (Range<UInt32>) async throws -> [Bool]
    ) async rethrows -> Int {
        guard maxCounter > 0, batchSize > 0 else { return 0 }

        var batchStart: UInt32 = 0
        var free = 0

        while batchStart < maxCounter {
            let batchEnd = min(batchStart + batchSize, maxCounter)
            let consumed = try await consumedInBatch(batchStart ..< batchEnd)
            free += consumed.lazy.filter { !$0 }.count

            // A free counter at the end of the batch means the whole remaining tail is free.
            if consumed.last == false {
                return free + Int(maxCounter - batchEnd)
            }

            batchStart = batchEnd
        }

        return free
    }
}

private extension UnloadQuotaTracker {
    func walk(periods: [UInt32], maxCounter: UInt32) async throws -> UnloadQuota {
        guard maxCounter > 0 else { return UnloadQuota(remaining: 0, limit: 0) }

        let pickedPerson = try await personOriginProvider.pickPersonOrigin()
        let aliasProvider = pickedPerson.makeAliasProvider()

        var remaining = 0
        for period in periods {
            remaining += try await Self.countFreeCounters(
                maxCounter: maxCounter,
                batchSize: Self.batchSize
            ) { range in
                let queries: [(period: UInt32, alias: Data)] = try range.map { counter in
                    let context = UnloadTokenContextBuilder.freeUnloadTokenContext(period: period, counter: counter)
                    return try (period: period, alias: aliasProvider.deriveAlias(for: context))
                }
                return try await consumedTokenChecker.fetchConsumedStatus(for: queries)
            }
        }

        return UnloadQuota(remaining: remaining, limit: Int(maxCounter) * periods.count)
    }
}
