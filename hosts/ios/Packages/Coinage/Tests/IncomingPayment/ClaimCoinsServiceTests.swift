import Testing
import AsyncExtensions
import Foundation
import NovaCrypto
import os
import StructuredConcurrencyTestSupport
@testable import Coinage

@Suite(.timeLimit(.minutes(5)))
struct ClaimCoinsServiceTests {
    private static let detectionTimeout: Duration = .seconds(30)
    private static let resubscribeDelay: Duration = .seconds(1)

    private static let denomination = DenominationBreakdownContext(
        unit: 1,
        precision: 0,
        maxExponent: 3,
        minExponent: 0
    )

    @Test
    func passWithoutLookPastWindowKeepsWaiting() async throws {
        let rig = try Rig(windowFromNow: -1)
        let run = rig.start()

        for _ in 0 ..< 3 {
            await rig.clock.resumeSleep(for: Self.detectionTimeout)
        }
        await rig.clock.waitForSleep(for: Self.detectionTimeout)

        #expect(!run.isFinished)
        #expect(rig.submitter.submittedPublicKeys.isEmpty)
        #expect(run.detectionsSoFar == [.detecting])
        run.cancel()
    }

    @Test
    func firstLookPastWindowIsClaimedThenWindowCloses() async throws {
        let rig = try Rig(windowFromNow: -1)
        let run = rig.start()

        await rig.clock.resumeSleep(for: Self.detectionTimeout)
        // A second pass proves the silent one past the window did not end the claim.
        await rig.clock.waitForSleep(for: Self.detectionTimeout)
        await rig.query.waitForSubscriptions(1)
        rig.query.send([rig.coinPublicKey: Self.coinInfo])
        await rig.submitter.waitForSubmissions(1)

        await rig.clock.resumeSleep(for: Self.detectionTimeout)
        let detections = await run.detections()

        #expect(rig.submitter.submittedPublicKeys == [[rig.coinPublicKey]])
        #expect(detections.last == .notClaimed)
    }

    @Test
    func lookInsideWindowLetsLaterSilentPassClose() async throws {
        let rig = try Rig(windowFromNow: 60)
        let run = rig.start()

        await rig.query.waitForSubscriptions(1)
        rig.query.send([rig.coinPublicKey: Self.coinInfo])
        await rig.submitter.waitForSubmissions(1)

        await rig.clock.waitForSleep(for: Self.detectionTimeout)
        rig.moveNow(by: 120)
        await rig.clock.resumeSleep(for: Self.detectionTimeout)

        #expect(await run.detections().last == .notClaimed)
    }

    @Test
    func failedSubscriptionIsReopened() async throws {
        let rig = try Rig(windowFromNow: -1)
        let run = rig.start()

        await rig.query.waitForSubscriptions(1)
        rig.query.fail()
        await rig.clock.resumeSleep(for: Self.resubscribeDelay)
        await rig.query.waitForSubscriptions(2)
        rig.query.send([rig.coinPublicKey: Self.coinInfo])
        await rig.submitter.waitForSubmissions(1)

        #expect(rig.submitter.submittedPublicKeys == [[rig.coinPublicKey]])
        run.cancel()
    }

    @Test
    func emptyLookInsideWindowKeepsPolling() async throws {
        let rig = try Rig(windowFromNow: 60)
        let run = rig.start()

        await rig.query.waitForSubscriptions(1)
        rig.query.send([:])
        for _ in 0 ..< 3 {
            await rig.clock.resumeSleep(for: Self.detectionTimeout)
        }
        await rig.clock.waitForSleep(for: Self.detectionTimeout)

        #expect(!run.isFinished)
        #expect(run.detectionsSoFar == [.detecting, .noCoinsOnChain])
        run.cancel()
    }

    @Test
    func coinAppearingAfterEmptyLookIsClaimed() async throws {
        let rig = try Rig(windowFromNow: 60)
        let run = rig.start()

        await rig.query.waitForSubscriptions(1)
        rig.query.send([:])
        // Whichever pass consumes the empty look reports it once that pass times out; time passes out
        // until it has, so the coin cannot answer a look that is still waiting on the empty one.
        repeat {
            await rig.clock.resumeSleep(for: Self.detectionTimeout)
            await rig.clock.waitForSleep(for: Self.detectionTimeout)
        } while run.detectionsSoFar.count < 2
        // Stands in for the entry the submission will register, seeded while the loop is inside its look so
        // the pass after the submission reads it; seeded later it would miss that pass's group read.
        rig.txService.seedClaim("claim-coins-test", receivedPublicKey: rig.coinPublicKey, status: .pending)
        rig.query.send([rig.coinPublicKey: Self.coinInfo])
        await rig.submitter.waitForSubmissions(1)
        await run.waitForDetections(3)

        #expect(run.detectionsSoFar == [.detecting, .noCoinsOnChain, .claiming])
        run.cancel()
    }

    @Test
    func emptyLookWithNothingClaimedEndsNotClaimed() async throws {
        let rig = try Rig(windowFromNow: 60)
        let run = rig.start()

        await rig.query.waitForSubscriptions(1)
        rig.query.send([:])
        await rig.clock.resumeSleep(for: Self.detectionTimeout)
        await rig.clock.waitForSleep(for: Self.detectionTimeout)
        rig.moveNow(by: 120)
        await rig.clock.resumeSleep(for: Self.detectionTimeout)
        let detections = await run.detections()

        #expect(detections.last == .notClaimed)
    }

    @Test
    func emptyLookWithSettledClaimDoesNotReportNoCoinsOnChain() async throws {
        let rig = try Rig(windowFromNow: 60, coinCount: 2)
        rig.txService.seedClaim(
            "claim-coins-test",
            receivedPublicKey: rig.coinPublicKeys[1],
            status: .finalizedSuccess
        )
        let run = rig.start()

        await rig.query.waitForSubscriptions(1)
        rig.query.send([:])
        for _ in 0 ..< 3 {
            await rig.clock.resumeSleep(for: Self.detectionTimeout)
        }
        await rig.clock.waitForSleep(for: Self.detectionTimeout)

        #expect(!run.isFinished)
        #expect(!run.detectionsSoFar.contains(.noCoinsOnChain))
        #expect(run.detectionsSoFar.allSatisfy { $0 == .detecting || $0 == .claiming })
        run.cancel()
    }

    @Test
    func foreignBundleEndsNotClaimedInsideWindow() async throws {
        let rig = try Rig(windowFromNow: 60)
        let run = rig.start()

        await rig.query.waitForSubscriptions(1)
        rig.query.send(CoinInfosSnapshot(foreign: [rig.coinPublicKey: Self.foreignInstance]))
        let detections = await run.detections()

        #expect(rig.submitter.submittedPublicKeys.isEmpty)
        #expect(detections.last == .notClaimed)
    }

    @Test
    func coinReadForeignIsNeverClaimedWhenItLaterReadsPresent() async throws {
        let rig = try Rig(windowFromNow: 60, coinCount: 2)
        let foreignKey = rig.coinPublicKeys[0]
        let ownKey = rig.coinPublicKeys[1]
        let run = rig.start()

        await rig.query.waitForSubscriptions(1)
        rig.query.send(CoinInfosSnapshot(foreign: [foreignKey: Self.foreignInstance]))
        rig.query.send(CoinInfosSnapshot(claimable: [foreignKey: Self.coinInfo, ownKey: Self.coinInfo]))
        await rig.submitter.waitForSubmissions(1)

        #expect(rig.submitter.submittedPublicKeys == [[ownKey]])
        run.cancel()
    }
}

private extension ClaimCoinsServiceTests {
    static let coinInfo = ClaimableCoinInfo(exponent: 0, age: 0)
    static let foreignInstance: CoinageInstanceId = 99

    /// One claim of a single coin against a scripted chain, on a manual clock and a movable date.
    struct Rig {
        let clock: ManualClock
        let query: ScriptedCoinInfoQuery
        let submitter: RecordingClaimSubmitter
        let txService: StubGroupTxService
        let coinSecrets: [Data]
        let coinPublicKeys: [Data]
        let retryUntil: Date

        var coinPublicKey: Data { coinPublicKeys[0] }

        private let dateProvider: MovableDateProvider
        private let service: ClaimCoinsService

        init(windowFromNow: TimeInterval, coinCount: Int = 1) throws {
            let keyFactory = SNKeyFactory()
            let keypairs = try (0 ..< coinCount).map { index in
                try keyFactory.createKeypair(fromSeed: Data(repeating: 0xAB &+ UInt8(index), count: 32))
            }
            let start = Date()
            let dateProvider = MovableDateProvider(now: start)
            let clock = ManualClock()
            let query = ScriptedCoinInfoQuery()
            let submitter = RecordingClaimSubmitter()
            let txService = StubGroupTxService()

            service = ClaimCoinsService(
                txService: txService,
                coinOnChainQuery: query,
                claimSubmitter: submitter,
                snKeyFactory: keyFactory,
                coinService: StubCoinService(),
                timing: ClaimCoinsService.Timing(
                    detectionTimeout: ClaimCoinsServiceTests.detectionTimeout,
                    resubscribeDelay: ClaimCoinsServiceTests.resubscribeDelay,
                    clock: clock,
                    dateProvider: dateProvider
                ),
                logger: StubLogger()
            )
            self.clock = clock
            self.query = query
            self.submitter = submitter
            self.dateProvider = dateProvider
            self.txService = txService
            coinSecrets = keypairs.map { $0.privateKey().rawData() }
            coinPublicKeys = keypairs.map { $0.publicKey().rawData() }
            retryUntil = start.addingTimeInterval(windowFromNow)
        }

        func start() -> ClaimRun {
            ClaimRun(stream: service.claim(
                coinKeys: coinSecrets,
                groupId: "claim-coins-test",
                retryUntil: retryUntil,
                context: ClaimCoinsServiceTests.denomination
            ))
        }

        func moveNow(by interval: TimeInterval) {
            dateProvider.advance(by: interval)
        }
    }

    /// A claim in flight: collects its detections and says whether the stream has finished.
    final class ClaimRun: @unchecked Sendable {
        private let task: Task<[CoinageTransferDetection], Never>
        private let finished = OSAllocatedUnfairLock(initialState: false)
        private let collected = OSAllocatedUnfairLock(initialState: [CoinageTransferDetection]())
        private let arrivals = CountSignal()

        init(stream: AnyAsyncSequence<CoinageTransferDetection>) {
            let finished = finished
            let collected = collected
            let arrivals = arrivals
            task = Task {
                do {
                    for try await detection in stream {
                        collected.withLock { $0.append(detection) }
                        arrivals.increment()
                    }
                } catch {}
                finished.withLock { $0 = true }
                return collected.withLock { $0 }
            }
        }

        var isFinished: Bool { finished.withLock { $0 } }

        var detectionsSoFar: [CoinageTransferDetection] { collected.withLock { $0 } }

        func waitForDetections(_ count: Int) async {
            await arrivals.wait(for: count)
        }

        func detections() async -> [CoinageTransferDetection] { await task.value }

        func cancel() { task.cancel() }
    }
}
