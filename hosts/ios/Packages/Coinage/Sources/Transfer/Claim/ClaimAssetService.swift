import AsyncExtensions
import BigInt
import Foundation
import KeyDerivation
import os
import SDKLogger
import StructuredConcurrency
import SubstrateSdk

/// Claims an external asset a peer funded into a wallet we hold the key to, by loading recycler
/// vouchers for it.
///
/// Like `ClaimCoinsService`, it is driven off the durability group registered under `groupId` and
/// retries until the full amount is claimed or `retryUntil` passes: it first awaits any operations
/// already registered for the group to settle (so a resumed claim watches a prior run's load instead
/// of re-submitting it), then, while short of the amount, detects newly-arrived balance and loads
/// vouchers for the remainder. Completion of the flow tells a caller the payment is finished.
public protocol ClaimAssetServicing: Sendable {
    func claim(
        wallet: any WalletManaging,
        amount: Balance,
        groupId: CoinageTxGroupId,
        retryUntil: Date,
        instanceId: CoinageInstanceId,
        context: DenominationBreakdownContext
    ) -> AnyAsyncSequence<CoinageTransferDetection>
}

public final class ClaimAssetService: ClaimAssetServicing, @unchecked Sendable {
    /// The pacing of one claim and the time it runs on. Injected so tests drive every timeout, delay,
    /// and window from a test clock instead of waiting on the wall clock.
    struct Timing: Sendable {
        /// How long one pass waits for a fresh balance look before loading against the last one seen.
        /// This is the only pacing between attempts, so a failing load retries at this cadence.
        let detectionTimeout: Duration
        /// How long a dropped balance subscription waits before it is opened again.
        let resubscribeDelay: Duration
        /// Paces the timeout and the delay above.
        let clock: any Clock<Duration>
        /// The wall-clock time the retry window is checked against. Kept as a date rather than a clock
        /// instant so a window opened on one launch means the same thing on the next.
        let now: @Sendable () -> Date

        static let production = Timing(
            detectionTimeout: .seconds(30),
            resubscribeDelay: .seconds(1),
            clock: ContinuousClock(),
            now: { Date() }
        )
    }

    private let assetsTracking: any AssetsTracking
    private let voucherLoaderFactory: any VoucherLoaderFactoryProtocol
    private let voucherService: any VoucherServiceProtocol
    private let txService: any CoinageTxServicing
    private let timing: Timing
    private let logger: SDKLoggerProtocol?

    init(
        assetsTracking: any AssetsTracking,
        voucherLoaderFactory: any VoucherLoaderFactoryProtocol,
        voucherService: any VoucherServiceProtocol,
        txService: any CoinageTxServicing,
        timing: Timing = .production,
        logger: SDKLoggerProtocol?
    ) {
        self.assetsTracking = assetsTracking
        self.voucherLoaderFactory = voucherLoaderFactory
        self.voucherService = voucherService
        self.txService = txService
        self.timing = timing
        self.logger = logger
    }

    public func claim(
        wallet: any WalletManaging,
        amount: Balance,
        groupId: CoinageTxGroupId,
        retryUntil: Date,
        instanceId: CoinageInstanceId,
        context: DenominationBreakdownContext
    ) -> AnyAsyncSequence<CoinageTransferDetection> {
        AsyncStream { continuation in
            let task = Task {
                let run = ClaimRun(
                    wallet: wallet,
                    amount: amount,
                    groupId: groupId,
                    retryUntil: retryUntil,
                    context: context,
                    report: { continuation.yield($0) }
                )
                await self.runClaim(run, instanceId: instanceId)
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }
}

// MARK: - Claim loop

private extension ClaimAssetService {
    /// One claim's fixed inputs, so the loop's helpers take the run rather than six parameters.
    struct ClaimRun {
        let wallet: any WalletManaging
        let amount: Balance
        let groupId: CoinageTxGroupId
        let retryUntil: Date
        let context: DenominationBreakdownContext
        let report: @Sendable (CoinageTransferDetection) -> Void
    }

    func runClaim(_ run: ClaimRun, instanceId: CoinageInstanceId) async {
        run.report(.detecting)

        guard let accountId = try? run.wallet.getRawPublicKey() else {
            logger?.error("Claim asset: invalid wallet for group=\(run.groupId)")
            run.report(.notClaimed)
            return
        }

        // Balance looks are consumed once, so a failing load cannot spin the loop off the same look.
        let balanceLooks = AsyncBufferedChannel<Balance>()
        let pump = Task { await self.pumpBalance(into: balanceLooks, instanceId: instanceId, accountId: accountId) }
        defer { pump.cancel() }

        logger?.debug("Will start claim for group=\(run.groupId) amount=\(run.amount)")

        do {
            let settled = try await claimUntilDone(run, looks: balanceLooks.makeAsyncIterator())
            let verdict = try await toVerdict(settled, amount: run.amount, context: run.context)

            // A cancelled run has no last word: the record stays active for the next launch.
            guard !Task.isCancelled else { return }

            logger?.debug("Claiming completed: verdict=\(verdict) group=\(run.groupId)")
            run.report(verdict)
        } catch {
            logger?.error("Claim asset: run ended without a verdict group=\(run.groupId): \(error)")
        }
    }

    /// Loads against arriving balance until the amount is claimed, the remainder is unloadable, or
    /// the window closes. Returns the group as it last settled. Throws when the group's value cannot
    /// be read: that is never a shortfall.
    func claimUntilDone(
        _ run: ClaimRun,
        looks: AsyncBufferedChannel<Balance>.Iterator
    ) async throws -> [CoinageTxEntry] {
        var settled: [CoinageTxEntry] = []
        var lastSeen: Balance = 0
        // Whether this run has taken a look at all. Tracked rather than inferred from `lastSeen`,
        // which stays 0 when nothing has arrived and so cannot tell "not looked yet" from "looked
        // and saw nothing" — the very distinction the window check depends on.
        var hasLooked = false

        while !Task.isCancelled {
            settled = try await awaitKnownOperationsSettled(run)

            let claimed = try await valueClaimed(settled.finalizedSuccess(), context: run.context)
            guard let remaining = remainder(after: claimed, run: run, hasLooked: hasLooked) else { break }

            lastSeen = await awaitBalance(looks, lastSeen: lastSeen, target: remaining)
            hasLooked = true
            // Deliberate rounding: a claim takes what the denominations can express and leaves the
            // dust, which no later look can make loadable either.
            let loadable = run.context.roundedDown(amountInPlanks: Swift.min(lastSeen, remaining))

            guard loadable > 0 else { continue }

            run.report(.claiming)
            logger?.debug("Claiming \(loadable) for group=\(run.groupId)")
            await load(wallet: run.wallet, amount: loadable, groupId: run.groupId, context: run.context)
        }

        return settled
    }

    /// What is still owed, or `nil` once nothing further will be attempted: the amount is covered, the
    /// remainder is below the smallest denomination (nothing can ever load it, however much more
    /// arrives), or the window closed *after* this run has looked.
    ///
    /// The window used to be checked before looking, on the reasoning that the funds are the caller's
    /// own and nothing else will spend them. That holds only while the key survives — and `settle`
    /// retires the source secret on a `notClaimed` verdict, which for a `.coins` source *is* the money.
    /// So a resume arriving past the window still takes one look and one attempt, the same guarantee
    /// ``ClaimCoinsService`` documents and makes.
    func remainder(after claimed: Balance, run: ClaimRun, hasLooked: Bool) -> Balance? {
        let remaining = run.amount > claimed ? run.amount - claimed : 0

        logger?.debug("Claimed \(claimed) of \(run.amount), remaining \(remaining) for group=\(run.groupId)")

        if remaining == 0 { return nil }

        if run.context.roundedDown(amountInPlanks: remaining) == 0 {
            logger?.debug("Claim asset: remainder \(remaining) is unloadable group=\(run.groupId)")
            return nil
        }

        if hasLooked, timing.now() >= run.retryUntil {
            logger?.debug("Claim asset: window closed group=\(run.groupId) claimed=\(claimed)")
            return nil
        }

        return remaining
    }

    /// Reports the group on every ledger update until nothing in it is live, then returns what it
    /// settled on — mirrors `ClaimCoinsService.awaitKnownOperationsSettled`. An empty group returns
    /// immediately (`allSatisfy` over no entries is `true`), the "not loaded yet" signal. A stream
    /// that fails settles on the last states seen; a valuation failure propagates.
    func awaitKnownOperationsSettled(_ run: ClaimRun) async throws -> [CoinageTxEntry] {
        var last: [CoinageTxEntry] = []
        do {
            for try await states in txService.subscribeOperationGroupStatuses(run.groupId) {
                last = states
                try await run.report(toProgress(states, amount: run.amount, context: run.context))
                if states.allSatisfy({ !$0.status.isLive }) { break }
            }
        } catch let error as ClaimValuationError {
            throw error
        } catch {
            logger?.error("Claim asset: group-status stream failed group=\(run.groupId): \(error)")
        }
        return last
    }

    /// Loads recycler vouchers for `amount` under `groupId`. The durability layer retries failed
    /// batches, so callers derive value from the settled group rather than this call's success.
    func load(
        wallet: any WalletManaging,
        amount: Balance,
        groupId: CoinageTxGroupId,
        context: DenominationBreakdownContext
    ) async {
        do {
            let loader = try voucherLoaderFactory.makeLoader(for: wallet)
            _ = try await loader.load(amount: amount, breakdownContext: context, groupId: groupId)
        } catch {
            logger?.error("Claim asset: voucher load failed group=\(groupId): \(error)")
        }
    }
}

// MARK: - Detection

private extension ClaimAssetService {
    /// Feeds every balance look into `channel` until cancelled, reopening the subscription after
    /// ``Timing/resubscribeDelay`` whenever it fails or ends. A tracker that cannot be opened at all
    /// therefore feeds nothing, and the loop runs out its window on the last balance seen.
    func pumpBalance(
        into channel: AsyncBufferedChannel<Balance>,
        instanceId: CoinageInstanceId,
        accountId: AccountId
    ) async {
        while !Task.isCancelled {
            do {
                let stream = try await assetsTracking.track(instanceId: instanceId, accountId: accountId)
                for try await balance in stream {
                    channel.send(balance)
                }
            } catch {
                logger?.error("Claim asset: balance tracking failed for instance=\(instanceId): \(error)")
            }

            guard await (try? timing.clock.sleep(for: timing.resubscribeDelay)) != nil else { break }
        }
        channel.finish()
    }

    /// The newest balance look within ``Timing/detectionTimeout``, stopping early once it covers
    /// `target`; `lastSeen` when no fresh look arrives in time. The newest look wins outright even when
    /// smaller: the account can be spent from elsewhere.
    func awaitBalance(
        _ looks: AsyncBufferedChannel<Balance>.Iterator,
        lastSeen: Balance,
        target: Balance
    ) async -> Balance {
        let latest = OSAllocatedUnfairLock<Balance>(initialState: lastSeen)
        _ = try? await withTimeout(timing.detectionTimeout, clock: timing.clock) {
            while let balance = await looks.next() {
                latest.withLock { $0 = balance }
                if balance >= target { break }
            }
        }
        return latest.withLock { $0 }
    }
}

// MARK: - Detection verdicts

private extension ClaimAssetService {
    /// What is true right now, reported on every ledger update. A shortfall is never announced here —
    /// only `toVerdict` may say claiming is over — so `claimed` is surfaced only once the full amount
    /// has arrived.
    func toProgress(
        _ states: [CoinageTxEntry],
        amount: Balance,
        context: DenominationBreakdownContext
    ) async throws -> CoinageTransferDetection {
        guard !states.isEmpty else { return .detecting }
        guard states.allSatisfy(\.status.isArrived) else { return .claiming }

        let value = try await valueClaimed(states, context: context)
        guard value >= amount else { return .claiming }

        let finalized = states.allSatisfy { $0.status == .finalizedSuccess }
        return .claimed(amount: value, finalized: finalized)
    }

    /// The last word, once nothing further will be attempted. Success only when the full amount
    /// finalized; a lesser finalized value that still claimed something is partial (source
    /// underfunded), matching the host spec.
    func toVerdict(
        _ states: [CoinageTxEntry],
        amount: Balance,
        context: DenominationBreakdownContext
    ) async throws -> CoinageTransferDetection {
        let value = try await valueClaimed(states.finalizedSuccess(), context: context)
        return .verdict(finalized: value, of: amount)
    }

    /// The planks loaded by `entries` — their output vouchers, fetched by key, valued against the
    /// denomination context. A store that cannot be read is a `ClaimValuationError`, never zero.
    func valueClaimed(
        _ entries: [CoinageTxEntry],
        context: DenominationBreakdownContext
    ) async throws -> Balance {
        let outputKeys = entries.outputPublicKeys()
        guard !outputKeys.isEmpty else { return 0 }

        do {
            let vouchers = try await voucherService.fetchVouchers(publicKeys: outputKeys)
            return vouchers.reduce(Balance(0)) { $0 + context.valueInPlanks(for: $1.exponent) }
        } catch {
            throw ClaimValuationError(underlying: error)
        }
    }
}
