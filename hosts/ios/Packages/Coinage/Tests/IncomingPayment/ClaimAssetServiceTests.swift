import Testing
import AsyncExtensions
import Foundation
import KeyDerivation
import os
import SubstrateSdk
@testable import Coinage

/// Drives `ClaimAssetService` against a fake chain: a stub balance tracker, a loader that registers
/// real durability entries with `MockCoinageTxService`, and an in-memory voucher store to value them.
/// Time is virtual (`TestTime`): every timeout, delay, and window is driven by the test, so the exact
/// retry cadence is asserted rather than a wall-clock range.
///
/// Every virtual step costs dozens of task yields, so wall time scales with how starved the process is:
/// a CI runner measured 28x slower than usual pushed the longest tests here past one minute while the
/// virtual `Stalled` limit stays the real hang tripwire.
@Suite(.timeLimit(.minutes(5)))
struct ClaimAssetServiceTests {
    /// Denominations 8, 4, 2, 1 planks.
    private static let denomination = DenominationBreakdownContext(
        unit: 1,
        precision: 0,
        maxExponent: 3,
        minExponent: 0
    )

    /// Denominations 8, 4, 2 planks — 1 plank can never be loaded.
    private static let coarseDenomination = DenominationBreakdownContext(
        unit: 1,
        precision: 0,
        maxExponent: 3,
        minExponent: 1
    )

    private static let detectionTimeout: Duration = .milliseconds(50)
    private static let resubscribeDelay: Duration = .milliseconds(10)

    private static let wallet = DynamicDerivedWallet(
        derivationPath: "//topup",
        entropyManager: MockEntropyManager(entropy: Data(repeating: 0x02, count: 32))
    )

    @Test
    func exactAmountIsClaimedAndFinalized() async throws {
        let rig = makeRig(tracking: StubAssetsTracking(looks: [10]))

        let detections = await rig.run(amount: 10, retryUntil: .distantFuture, context: Self.denomination)

        #expect(detections.first == .detecting)
        #expect(detections.contains(.claiming))
        #expect(detections.last == .claimed(amount: 10, finalized: true))
        #expect(rig.loader.loads() == [10])
    }

    @Test
    func remainderBelowSmallestDenominationEndsAsPartial() async throws {
        let rig = makeRig(tracking: StubAssetsTracking(looks: [11]))

        let detections = await rig.run(amount: 11, retryUntil: .distantFuture, context: Self.coarseDenomination)

        #expect(detections.last == .claimedPartially(claimed: 10))
        // The 1-plank remainder is not attempted again: nothing could ever load it. The loader is
        // asked for the 10 the denominations can carry, rather than for 11 and quietly given 10.
        #expect(rig.loader.loads() == [10])
    }

    @Test
    func nothingArrivingWithinWindowIsNotClaimed() async throws {
        let tracking = StubAssetsTracking(looks: [])
        let rig = makeRig(tracking: tracking)

        let run = rig.start(amount: 10, retryUntil: rig.time.date(after: 0.25), context: Self.denomination)
        try await rig.time.advance(until: { run.isFinished })

        let detections = await run.detections()
        #expect(detections.first == .detecting)
        #expect(!detections.contains(.claiming))
        #expect(detections.last == .notClaimed)
        #expect(rig.loader.loads().isEmpty)
        #expect(tracking.trackCalls() == 1)
    }

    /// Reversed deliberately (audit finding H5). This previously asserted that a closed window is never
    /// attempted "even when funded", on the reasoning that the funds are the caller's own and nothing
    /// else will spend them. That holds only while the key survives — and `settle` wipes the source
    /// secret on `notClaimed`, which for a `.coins` source *is* the money. A resume that arrives after
    /// the window must therefore still take one look and one attempt, the same guarantee
    /// `ClaimCoinsService` documents and makes.
    @Test
    func closedWindowStillTakesOneAttemptWhenFunded() async throws {
        let rig = makeRig(tracking: StubAssetsTracking(looks: [10]))

        let detections = await rig.run(
            amount: 10,
            retryUntil: rig.time.date(after: -1),
            context: Self.denomination
        )

        #expect(detections.first == .detecting)
        #expect(detections.contains(.claiming))
        #expect(detections.last == .claimed(amount: 10, finalized: true))
        #expect(rig.loader.loads() == [10])
    }

    /// The window still ends the loop — it just does so after the look, not before it. Nothing arriving
    /// means nothing to attempt, so no load is made and the verdict is unchanged.
    @Test
    func closedWindowWithNothingArrivingMakesNoAttempt() async throws {
        let rig = makeRig(tracking: StubAssetsTracking(looks: []))

        let run = rig.start(amount: 10, retryUntil: rig.time.date(after: -1), context: Self.denomination)
        try await rig.time.advance(until: { run.isFinished })

        #expect(await run.detections().last == .notClaimed)
        #expect(rig.loader.loads().isEmpty)
    }

    @Test
    func unopenableTrackerIsRetriedAtItsDelayUntilTheWindowCloses() async throws {
        let tracking = StubAssetsTracking(openError: StubAssetsTracking.Unavailable())
        let rig = makeRig(tracking: tracking)

        let run = rig.start(amount: 10, retryUntil: rig.time.date(after: 0.25), context: Self.denomination)

        // Each reopen needs the service scheduled, but the deadline is absolute: stepping the clock
        // without waiting for the reopen it triggers loses reopens outright and lands under the range
        // below. Advancing to each one in turn keeps the clock behind the service.
        var reopens = 1
        while !run.isFinished {
            let expected = reopens
            try await rig.time.advance(until: { tracking.trackCalls() >= expected || run.isFinished })
            reopens += 1
        }

        #expect(await run.detections().last == .notClaimed)
        #expect(rig.loader.loads().isEmpty)
        // Reopened once per resubscribe delay across the 250 ms window: 1 + 250 / 10, give or take the
        // reopen that races the window closing.
        #expect((25 ... 27).contains(tracking.trackCalls()))
    }

    @Test
    func failingLoadIsRetriedAtDetectionCadenceUntilTheWindowCloses() async throws {
        let rig = makeRig(tracking: StubAssetsTracking(looks: [10]))
        rig.loader.loadError = StubVoucherLoaderFactory.Failure()

        let run = rig.start(amount: 10, retryUntil: rig.time.date(after: 0.3), context: Self.denomination)

        // One attempt per detection timeout. The window is checked before each wait, not after it, so
        // the wait that ends exactly at the deadline still loads: t = 0, 50, ..., 300 ms. Stepping to
        // each load keeps the clock from running ahead of the service and dropping that last attempt.
        for expected in 1 ... 7 {
            try await rig.time.advance(until: { rig.loader.loads().count >= expected || run.isFinished })
        }
        try await rig.time.advance(until: { run.isFinished })

        #expect(await run.detections().last == .notClaimed)
        #expect(rig.loader.loads().count == 7)
    }

    @Test
    func batchFailingOnChainIsRetriedAtDetectionCadenceUntilTheWindowCloses() async throws {
        let rig = makeRig(tracking: StubAssetsTracking(looks: [10]), submissionOutcome: .chainFailure)

        let run = rig.start(amount: 10, retryUntil: rig.time.date(after: 0.3), context: Self.denomination)

        // One load lands immediately, then one per detection timeout at 50ms...300ms inclusive. The
        // 300ms attempt survives only because the window check preceding it reads exactly 250ms, so
        // the clock must not run ahead of the service: `advance(until:)` steps blind, and a single
        // run to completion can spend five steps while the service is descheduled and lose that last
        // attempt. Stopping at each load keeps the drift at one step.
        for expected in 1 ... 7 {
            try await rig.time.advance(until: { rig.loader.loads().count >= expected || run.isFinished })
        }
        try await rig.time.advance(until: { run.isFinished })

        #expect(await run.detections().last == .notClaimed)
        #expect(rig.loader.loads().count == 7)
    }

    @Test
    func fundsArrivingLaterAreClaimed() async throws {
        let tracking = StubAssetsTracking(looks: [0])
        let rig = makeRig(tracking: tracking)

        let run = rig.start(amount: 10, retryUntil: .distantFuture, context: Self.denomination)
        // A look sent before the claim has subscribed reaches nobody, so wait for the subscription,
        // let two detection timeouts pass with nothing to load, then land the funds.
        try await rig.time.advance(until: { tracking.trackCalls() == 1 })
        await rig.time.advance(by: .milliseconds(120))
        #expect(rig.loader.loads().isEmpty)
        tracking.send(10)
        try await rig.time.advance(until: { run.isFinished })

        #expect(await run.detections().last == .claimed(amount: 10, finalized: true))
        #expect(rig.loader.loads() == [10])
    }

    @Test
    func unreadableVoucherStoreEndsWithoutAVerdict() async throws {
        let rig = makeRig(tracking: StubAssetsTracking(looks: [10]))
        rig.vouchers.fetchError = InMemoryVoucherService.Unreadable()

        let detections = await rig.run(amount: 10, retryUntil: .distantFuture, context: Self.denomination)

        // The load went through, but its value cannot be read: that is not a shortfall, so the run
        // ends on a non-terminal detection and leaves the verdict to a launch that can read the store.
        #expect(rig.loader.loads() == [10])
        #expect(detections.last == .claiming)
        #expect(!detections.contains(.notClaimed))
        #expect(!detections.contains { if case .claimedPartially = $0 { true } else { false } })
    }

    struct Rig {
        let service: ClaimAssetService
        let loader: StubVoucherLoaderFactory
        let vouchers: InMemoryVoucherService
        let time: TestTime

        /// Runs a claim to completion. Only for claims that finish without the clock moving.
        func run(
            amount: Balance,
            retryUntil: Date,
            context: DenominationBreakdownContext
        ) async -> [CoinageTransferDetection] {
            await start(amount: amount, retryUntil: retryUntil, context: context).detections()
        }

        /// Starts a claim the test then drives by advancing `time`.
        func start(
            amount: Balance,
            retryUntil: Date,
            context: DenominationBreakdownContext
        ) -> ClaimRun {
            ClaimRun(stream: service.claim(
                wallet: ClaimAssetServiceTests.wallet,
                amount: amount,
                groupId: "top up:prod:p",
                retryUntil: retryUntil,
                instanceId: 0,
                context: context
            ))
        }
    }

    /// A claim in flight: collects its detections and says when the stream has finished.
    final class ClaimRun: @unchecked Sendable {
        private let task: Task<[CoinageTransferDetection], Never>
        private let finished = OSAllocatedUnfairLock(initialState: false)

        init(stream: AnyAsyncSequence<CoinageTransferDetection>) {
            let finished = finished
            task = Task {
                var detections: [CoinageTransferDetection] = []
                do {
                    for try await detection in stream {
                        detections.append(detection)
                    }
                } catch {}
                finished.withLock { $0 = true }
                return detections
            }
        }

        var isFinished: Bool { finished.withLock { $0 } }

        func detections() async -> [CoinageTransferDetection] { await task.value }
    }

    private func makeRig(
        tracking: StubAssetsTracking,
        submissionOutcome: MockCoinageTxService.SubmissionOutcome = .success
    ) -> Rig {
        let time = TestTime()
        let txService = MockCoinageTxService(submissionOutcome: submissionOutcome)
        let vouchers = InMemoryVoucherService()
        let loader = StubVoucherLoaderFactory(vouchers: vouchers, txService: txService)
        let service = ClaimAssetService(
            assetsTracking: tracking,
            voucherLoaderFactory: loader,
            voucherService: vouchers,
            txService: txService,
            timing: ClaimAssetService.Timing(
                detectionTimeout: Self.detectionTimeout,
                resubscribeDelay: Self.resubscribeDelay,
                clock: time.clock,
                now: { time.now }
            ),
            logger: StubLogger()
        )
        return Rig(service: service, loader: loader, vouchers: vouchers, time: time)
    }
}
