import AsyncExtensions
import BigInt
import Foundation
import FoundationExt
import NovaCrypto
import os
import SDKLogger
import StructuredConcurrency
import SubstrateSdk

/// Claims coins a peer handed us, driven entirely off the durability group registered under
/// `groupId`.
///
/// Claiming is not one-shot: a coin the chain still shows unclaimed is money the peer has parted
/// with that nothing else will collect, so a failed claim is retried whenever the coin is still
/// there — until every coin has a finalized claim, or `retryUntil` passes. The window ends the loop
/// only once the run has looked at the chain, so a run resumed past its window — or started offline —
/// still gets one attempt with a live connection.
///
/// The flow completes only when nothing further will be attempted, so its completion is what tells a
/// caller the payment is finished — no interim detection means that.
public protocol ClaimCoinsServicing: Sendable {
    func claim(
        coinKeys: [Data],
        groupId: CoinageTxGroupId,
        retryUntil: Date,
        context: DenominationBreakdownContext
    ) -> AnyAsyncSequence<CoinageTransferDetection>
}

public final class ClaimCoinsService: ClaimCoinsServicing, @unchecked Sendable {
    /// The pacing of one claim and the time it runs on. Injected so tests drive every timeout, delay,
    /// and window from a test clock instead of waiting on the wall clock.
    struct Timing: Sendable {
        /// How long one pass waits for every coin to show up before claiming whatever it can see. A
        /// peer's coin has no ledger row of ours, so only the chain can say it exists, and one that never
        /// arrives must not hold up the ones that did.
        let detectionTimeout: Duration
        /// How long a dropped coin subscription waits before it is opened again.
        let resubscribeDelay: Duration
        /// Paces the timeout and the delay above.
        let clock: any Clock<Duration>
        /// The wall-clock time the retry window is checked against. A date rather than a clock instant,
        /// so a window opened on one launch means the same thing on the next.
        let dateProvider: any DateProviding

        static let production = Timing(
            detectionTimeout: .seconds(30),
            resubscribeDelay: .seconds(1),
            clock: ContinuousClock(),
            dateProvider: NowDateProvider()
        )
    }

    private let txService: any CoinageTxServicing
    private let coinOnChainQuery: any CoinOnChainQuerying
    private let claimSubmitter: any CoinageClaimSubmitting
    private let snKeyFactory: any SNKeyFactoryProtocol
    private let coinService: any CoinServiceProtocol
    private let timing: Timing
    private let logger: SDKLoggerProtocol?

    init(
        txService: any CoinageTxServicing,
        coinOnChainQuery: any CoinOnChainQuerying,
        claimSubmitter: any CoinageClaimSubmitting,
        snKeyFactory: any SNKeyFactoryProtocol,
        coinService: any CoinServiceProtocol,
        timing: Timing = .production,
        logger: SDKLoggerProtocol?
    ) {
        self.txService = txService
        self.coinOnChainQuery = coinOnChainQuery
        self.claimSubmitter = claimSubmitter
        self.snKeyFactory = snKeyFactory
        self.coinService = coinService
        self.timing = timing
        self.logger = logger
    }

    public func claim(
        coinKeys: [Data],
        groupId: CoinageTxGroupId,
        retryUntil: Date,
        context: DenominationBreakdownContext
    ) -> AnyAsyncSequence<CoinageTransferDetection> {
        AsyncStream { continuation in
            let task = Task {
                await self.runClaim(
                    coinKeys: coinKeys,
                    groupId: groupId,
                    retryUntil: retryUntil,
                    context: context
                ) { detection in
                    continuation.yield(detection)
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }
}

// MARK: - Claim loop

private extension ClaimCoinsService {
    /// What one pass saw of the coins still owed, and whether it saw the chain at all.
    struct CoinsLook {
        let claimable: [PublicKey: ClaimableCoinInfo]
        let looked: Bool
    }

    func runClaim(
        coinKeys: [Data],
        groupId: CoinageTxGroupId,
        retryUntil: Date,
        context: DenominationBreakdownContext,
        report: @Sendable (CoinageTransferDetection) -> Void
    ) async {
        report(.detecting)

        let keypairs = deriveKeypairs(from: coinKeys)
        let coins = Set(keypairs.keys)
        guard !coins.isEmpty else { report(.notClaimed); return }

        let onChainUpdates = AsyncBufferedChannel<[PublicKey: ClaimableCoinInfo]>()
        let pump = Task {
            await self.pumpCoinInfos(into: onChainUpdates, coins: coins, groupId: groupId)
        }
        defer { pump.cancel() }

        logger?.debug("Will start claiming coins with group=\(groupId)")

        do {
            let settled = try await claimUntilDone(
                keypairs: keypairs,
                groupId: groupId,
                retryUntil: retryUntil,
                context: context,
                onChain: onChainUpdates.makeAsyncIterator(),
                report: report
            )
            let verdict = try await toVerdict(settled, coins: coins, context: context)

            // A cancelled run has no last word: the record stays active for the next launch.
            guard !Task.isCancelled else { return }

            report(verdict)
        } catch {
            logger?.error("Claim: run ended without a verdict group=\(groupId): \(error)")
        }
    }

    /// Claims against the chain until every coin has a finalized claim or the window closes. Returns
    /// the group as it last settled. Throws when the group's value cannot be read: never a shortfall.
    func claimUntilDone(
        keypairs: [PublicKey: Data],
        groupId: CoinageTxGroupId,
        retryUntil: Date,
        context: DenominationBreakdownContext,
        onChain: AsyncBufferedChannel<[PublicKey: ClaimableCoinInfo]>.Iterator,
        report: @Sendable (CoinageTransferDetection) -> Void
    ) async throws -> [CoinageTxEntry] {
        let coins = Set(keypairs.keys)
        var settled: [CoinageTxEntry] = []
        var hasLooked = false

        while !Task.isCancelled {
            logger?.debug("Awaiting settles coins=\(coins.count) with group=\(groupId)")

            settled = try await awaitKnownOperationsSettled(
                groupId: groupId, coins: coins, context: context, report: report
            )
            // Each coin is registered once: rebuilding a claim that failed is its submission policy's
            // job, into the coin that claim recorded. A second claim here would mint into a coin
            // nothing would ever wait on.
            //
            // `receivedPublicKeys()` counts terminal entries too, so a coin whose policy has already
            // given up reads as registered and is not re-registered here. That is deliberate — see the
            // note on `CoinageClaimSubmitting` — and it means no layer retries such a coin: the policy
            // is dead and this loop treats it as handled. It stays on chain, claimable by nobody, until
            // the sender-side reclaim UI exists.
            let unregistered = coins.subtracting(settled.receivedPublicKeys())
            if unregistered.isEmpty {
                logger?.debug("Every coin has a claim group=\(groupId)")
                break
            }

            logger?.debug("Unregistered \(unregistered.count) coins for group=\(groupId)")

            let look = await awaitOnChainWithTimeout(onChain, unclaimed: unregistered)
            hasLooked = hasLooked || look.looked

            logger?.debug("Claimable \(look.claimable.count) coins for group=\(groupId)")

            if !look.claimable.isEmpty {
                await submit(
                    claimable: look.claimable,
                    keypairs: keypairs,
                    bundleSize: coins.count,
                    groupId: groupId,
                    retryUntil: retryUntil
                )
            } else if await isWindowClosed(retryUntil, hasLooked: hasLooked, groupId: groupId) {
                logger?.debug("Claim window closed group=\(groupId) unregistered=\(unregistered.count)")
                break
            }
        }

        return settled
    }

    /// Reports the group on every ledger update until nothing in it is live, then returns what it
    /// settled on. An empty group is already settled — a first attempt, nothing to wait for. A stream
    /// that fails settles on the last states seen; a valuation failure propagates.
    func awaitKnownOperationsSettled(
        groupId: CoinageTxGroupId,
        coins: Set<PublicKey>,
        context: DenominationBreakdownContext,
        report: @Sendable (CoinageTransferDetection) -> Void
    ) async throws -> [CoinageTxEntry] {
        var last: [CoinageTxEntry] = []
        do {
            for try await states in txService.subscribeOperationGroupStatuses(groupId) {
                last = states
                try await report(toProgress(states, coins: coins, context: context))
                if states.allSatisfy({ !$0.status.isLive }) { break }
            }
        } catch let error as ClaimValuationError {
            throw error
        } catch {
            logger?.error("Claim group-status stream failed group=\(groupId): \(error)")
        }
        return last
    }

    func submit(
        claimable: [PublicKey: ClaimableCoinInfo],
        keypairs: [PublicKey: Data],
        bundleSize: Int,
        groupId: CoinageTxGroupId,
        retryUntil: Date
    ) async {
        let items = claimable.compactMap { key, info -> ClaimableCoin? in
            guard let privateKey = keypairs[key] else { return nil }
            return ClaimableCoin(
                privateKey: privateKey,
                publicKey: key,
                valueExponent: info.exponent,
                age: info.age
            )
        }
        logger?.debug("Claiming coins for group=\(groupId)")

        do {
            try await claimSubmitter.submit(
                claimable: items,
                bundleSize: bundleSize,
                groupId: groupId,
                retryUntil: retryUntil
            )
            logger?.debug("Claiming complete for group=\(groupId)")
        } catch {
            logger?.error("Claim submission failed group=\(groupId): \(error)")
        }
    }

    /// The next look at the chain that shows every still-owed coin, or the best look within
    /// ``Timing/detectionTimeout``. Reads the consume-once channel one look at a time, so a failing
    /// submit cannot spin the loop. Settling for the last look is deliberate: a coin that never arrives
    /// is the peer's problem, and holding the others hostage to it would strand money sitting right there.
    func awaitOnChainWithTimeout(
        _ onChain: AsyncBufferedChannel<[PublicKey: ClaimableCoinInfo]>.Iterator,
        unclaimed: Set<PublicKey>
    ) async -> CoinsLook {
        let latest = OSAllocatedUnfairLock<CoinsLook>(initialState: CoinsLook(claimable: [:], looked: false))
        _ = try? await withTimeout(timing.detectionTimeout, clock: timing.clock) {
            while let look = await onChain.next() {
                let filtered = look.filter { unclaimed.contains($0.key) }
                latest.withLock { $0 = CoinsLook(claimable: filtered, looked: true) }
                if unclaimed.isSubset(of: Set(filtered.keys)) { break }
            }
        }
        return latest.withLock { $0 }
    }

    /// An empty pass is the window's last word only once the run has seen the chain: without a look,
    /// "nothing claimable" may just mean no connection.
    func isWindowClosed(_ retryUntil: Date, hasLooked: Bool, groupId: CoinageTxGroupId) async -> Bool {
        guard await timing.dateProvider.read() >= retryUntil else { return false }
        guard hasLooked else {
            logger?.debug("Claim window passed, awaiting first look group=\(groupId)")
            return false
        }
        return true
    }

    /// Feeds every look at the coins into `channel` until cancelled, reopening the subscription after
    /// ``Timing/resubscribeDelay`` whenever it fails or ends — a claim that waits for a look must not
    /// wait on a subscription that is gone.
    func pumpCoinInfos(
        into channel: AsyncBufferedChannel<[PublicKey: ClaimableCoinInfo]>,
        coins: Set<PublicKey>,
        groupId: CoinageTxGroupId
    ) async {
        while !Task.isCancelled {
            do {
                for try await snapshot in coinOnChainQuery.subscribeCoinInfos(for: Array(coins)) {
                    channel.send(snapshot)
                }
                guard !Task.isCancelled else { break }
                logger?.debug("Claim coin subscription ended group=\(groupId)")
            } catch {
                logger?.error("Claim coin subscription failed group=\(groupId): \(error)")
            }

            guard await (try? timing.clock.sleep(for: timing.resubscribeDelay)) != nil else { break }
        }
        channel.finish()
    }

    func deriveKeypairs(from coinKeys: [Data]) -> [PublicKey: Data] {
        var result: [PublicKey: Data] = [:]
        for key in coinKeys {
            guard let publicKey = try? snKeyFactory.createPublicKey(fromSecret: key).rawData() else {
                logger?.warning("Claim: failed to derive public key for a coin key")
                continue
            }
            result[publicKey] = key
        }
        return result
    }
}

// MARK: - Detection

private extension ClaimCoinsService {
    /// What is true right now, reported on every ledger update. Nothing here may say claiming is
    /// over — only the loop knows that — so a shortfall is never announced while a retry could make it up.
    func toProgress(
        _ states: [CoinageTxEntry],
        coins: Set<PublicKey>,
        context: DenominationBreakdownContext
    ) async throws -> CoinageTransferDetection {
        let arrived = states.filter(\.status.isArrived)
        let outstanding = coins.subtracting(arrived.receivedPublicKeys())

        if outstanding.isEmpty {
            let finalized = coins.subtracting(states.finalizedSuccess().receivedPublicKeys()).isEmpty
            return try await .claimed(amount: valueMinted(by: arrived, context: context), finalized: finalized)
        }

        // One waiting to be built again has not failed — announcing a shortfall there would call a
        // rebuild that may still land a loss.
        let failed = states.filter { $0.status == .failure }.receivedPublicKeys()
        if !arrived.isEmpty, outstanding.contains(where: { failed.contains($0) }) {
            return try await .claimingRest(claimed: valueMinted(by: arrived, context: context))
        }

        return states.isEmpty ? .detecting : .claiming
    }

    /// The last word, once nothing further will be attempted — the only place a shortfall may be final.
    func toVerdict(
        _ states: [CoinageTxEntry],
        coins: Set<PublicKey>,
        context: DenominationBreakdownContext
    ) async throws -> CoinageTransferDetection {
        let arrived = states.filter(\.status.isArrived)
        let notArrived = coins.subtracting(arrived.receivedPublicKeys())

        if notArrived.isEmpty {
            let finalized = coins.subtracting(states.finalizedSuccess().receivedPublicKeys()).isEmpty
            return try await .claimed(amount: valueMinted(by: arrived, context: context), finalized: finalized)
        }
        if !arrived.isEmpty {
            return try await .claimedPartially(claimed: valueMinted(by: arrived, context: context))
        }
        return .notClaimed
    }

    /// The planks minted by `entries` — their output coins valued against the denomination context.
    /// A store that cannot be read is a `ClaimValuationError`, never zero.
    func valueMinted(by entries: [CoinageTxEntry], context: DenominationBreakdownContext) async throws -> Balance {
        let outputKeys = entries.outputPublicKeys()

        guard !outputKeys.isEmpty else { return 0 }

        do {
            let coins = try await coinService.fetchCoins(publicKeys: outputKeys)
            return coins.reduce(Balance(0)) { $0 + context.valueInPlanks(for: $1.exponent) }
        } catch {
            throw ClaimValuationError(underlying: error)
        }
    }
}
