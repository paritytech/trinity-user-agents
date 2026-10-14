import Foundation
import SubstrateSdk
import SubstrateOperation
import KeyDerivation

/// Resolved unload token parameters ready for proof generation.
public struct ResolvedUnloadToken {
    /// The selected period for the unload token.
    public let period: UInt32
    /// The selected (unconsumed) counter within the period.
    public let counter: UInt32
    /// The VRF context bytes for the main proof.
    public let unloadTokenContext: Data

    public init(
        period: UInt32,
        counter: UInt32,
        unloadTokenContext: Data
    ) {
        self.period = period
        self.counter = counter
        self.unloadTokenContext = unloadTokenContext
    }
}

/// Protocol for resolving unload token parameters (period + counter) from chain state.
public protocol UnloadTokenResolving {
    /// Resolves periods and counters for multiple voucher groups in a single batch.
    ///
    /// Each group gets its own distinct (period, counter) pair, enabling concurrent
    /// extrinsic submission without conflict.
    ///
    /// - Parameters:
    ///   - groups: Array of voucher groups.
    ///   - aliasProvider: Abstract alias provider to form correct aliases for storage.
    ///   - currentDate: Injectable current date for testability.
    /// - Returns: Array of `ResolvedUnloadToken`, one per input group (same order).
    /// - Throws: `UnloadTokenResolverError.noAvailableCounter` if not enough counters available.
    func resolve(
        groups: [[Voucher]],
        aliasProvider: any AliasProviding,
        currentDate: Date
    ) async throws -> [ResolvedUnloadToken]
}

public final class UnloadTokenResolver {
    private let runtimeCodingService: RuntimeCodingServiceProtocol
    private let viewFunctionFetcher: ViewFunctionFetching
    private let consumedTokenChecker: any ConsumedTokenChecking

    public init(
        runtimeCodingService: RuntimeCodingServiceProtocol,
        viewFunctionFetcher: ViewFunctionFetching,
        consumedTokenChecker: any ConsumedTokenChecking
    ) {
        self.runtimeCodingService = runtimeCodingService
        self.viewFunctionFetcher = viewFunctionFetcher
        self.consumedTokenChecker = consumedTokenChecker
    }
}

// MARK: - UnloadTokenResolving

extension UnloadTokenResolver: UnloadTokenResolving {
    public func resolve(
        groups: [[Voucher]],
        aliasProvider: any AliasProviding,
        currentDate: Date
    ) async throws -> [ResolvedUnloadToken] {
        guard !groups.isEmpty else { return [] }

        let periodDuration: UInt64 = try await runtimeCodingService.fetchConstant(
            path: CoinagePallet.Constants.unloadTokenTimePeriod(),
            type: UInt64.self
        )

        // this is the upper bound
        // TODO: take person allowance into account
        let maxFreeTokens: StringCodable<UInt32> = try await viewFunctionFetcher.fetch(
            viewFunction: CoinagePallet.ViewFunction.maxFreeUnloadTokensPerTimePeriod()
        )
        let maxCounter = maxFreeTokens.wrappedValue

        let periods = UnloadTokenPeriodCalculator.validPeriods(
            currentDate: currentDate,
            periodDuration: periodDuration
        )

        var availableSlots: [(period: UInt32, counter: UInt32)] = []

        for period in periods {
            let slots = try await findAvailableSlots(
                period: period,
                maxCounter: maxCounter,
                aliasProvider: aliasProvider
            )
            availableSlots.append(contentsOf: slots)

            // Stop early if we already have enough slots for all groups
            if availableSlots.count >= groups.count { break }
        }

        guard availableSlots.count >= groups.count else {
            throw UnloadTokenResolverError.noAvailableCounter
        }

        return groups.enumerated().map { index, _ in
            let slot = availableSlots[index]

            let context = UnloadTokenContextBuilder.freeUnloadTokenContext(
                period: slot.period,
                counter: slot.counter
            )

            return ResolvedUnloadToken(
                period: slot.period,
                counter: slot.counter,
                unloadTokenContext: context
            )
        }
    }
}

// MARK: - Private

private extension UnloadTokenResolver {
    /// Returns all available (period, counter) slots for the given period.
    func findAvailableSlots(
        period: UInt32,
        maxCounter: UInt32,
        aliasProvider: any AliasProviding
    ) async throws -> [(period: UInt32, counter: UInt32)] {
        guard maxCounter > 0 else { return [] }

        let queries: [(period: UInt32, alias: Data)] = try (0 ..< maxCounter).map { counter in
            let context = UnloadTokenContextBuilder.freeUnloadTokenContext(
                period: period,
                counter: counter
            )
            let alias = try aliasProvider.deriveAlias(for: context)

            return (period: period, alias: alias)
        }

        let consumedStatuses = try await consumedTokenChecker.fetchConsumedStatus(for: queries)

        return consumedStatuses
            .enumerated()
            .filter { !$0.element }
            .map { (period: period, counter: UInt32($0.offset)) }
    }
}
