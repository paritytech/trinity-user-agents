import Coinage
import Foundation
import FoundationExt
import StructuredConcurrencyTestSupport
import SubstrateSdk
import Testing
@testable import polkadot_app

/// The monitor, the store and the chat snapshot together: a scripted claim must reach the message as
/// one emission per distinct state, a terminal row must keep the monitor away for good, and a runtime
/// failure must leave the row exactly as the durability layer last reported it.
@Suite("Coinage transfer monitor", .serialized)
struct CoinageTransferMonitorTests {
    private let context = DenominationBreakdownContext(unit: 1_000, precision: 3, maxExponent: 10, minExponent: 0)

    @Test("an incoming claim reaches the message once per distinct state")
    func incomingClaimReachesMessage() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming, totalValue: 10)
        let claims = MockClaimCoinsService(detections: [
            .detecting, .detecting, .claiming, .claimingRest(claimed: 4),
            .claimed(amount: 10, finalized: false), .claimed(amount: 10, finalized: true)
        ])
        let states = world.stateStream(of: message.messageId)

        let monitor = makeMonitor(world: world, claims: claims)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let seen = try await states.collect {
            $0 == .incoming(.init(status: .claimedFinalized, actualValue: 10))
        }
        let distinct = seen.enumerated().filter { $0.offset == 0 || seen[$0.offset - 1] != $0.element }.map(\.element)

        #expect(distinct == [
            nil,
            .incoming(.init(status: .detecting)),
            .incoming(.init(status: .claiming)),
            .incoming(.init(status: .claimed, actualValue: 10)),
            .incoming(.init(status: .claimedFinalized, actualValue: 10))
        ])
        #expect(seen.count == distinct.count, "every emission must carry a change")
        #expect(claims.calls.count == 1)
        #expect(claims.calls.first?.groupId == message.messageId)
    }

    @Test("a best-head claimed row is tracked again on the next launch until it is finalized")
    func bestHeadClaimedRowIsReprocessed() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming, totalValue: 10)
        try await world.store.updateIncoming(
            messageId: message.messageId,
            state: .init(status: .claimed, actualValue: 10)
        )
        let claims = MockClaimCoinsService(detections: [.claimed(amount: 10, finalized: true)])
        let states = world.stateStream(of: message.messageId)

        let monitor = makeMonitor(world: world, claims: claims)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        _ = try await states.collect { $0 == .incoming(.init(status: .claimedFinalized, actualValue: 10)) }
        #expect(claims.calls.count == 1)
    }

    @Test("a terminal row keeps the message out of the monitor on the next launch")
    func terminalRowIsNeverReprocessed() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)
        try await world.store.updateIncoming(
            messageId: message.messageId,
            state: .init(status: .claimedFinalized, actualValue: 10)
        )
        let claims = MockClaimCoinsService(detections: [.claimed(amount: 10, finalized: true)])

        let monitor = makeMonitor(world: world, claims: claims)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let candidates = world.messageProviderFactory.subscribeMessages(with: .incomingCoinageSendMessages())
        for try await messages in candidates {
            #expect(messages.isEmpty)
            break
        }
        #expect(claims.calls.isEmpty)
    }

    /// The failure log is the last statement of the monitor's catch, so once it has been written the
    /// task can no longer touch the row: what the row holds then is what it will hold.
    @Test("a runtime failure leaves the last durability-reported state in place")
    func runtimeFailureDoesNotWriteStatus() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)
        let claims = MockClaimCoinsService(detections: [.claiming], failure: MonitorTestError.chainUnavailable)
        let logger = MockLogger()

        let monitor = makeMonitor(world: world, claims: claims, logger: logger)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        try await logger.waitForError(containing: "Failed to claim coinage for \(message.messageId)")

        let transfer = try #require(try await world.transfer(message.messageId))
        #expect(transfer.state == .incoming(.init(status: .claiming)))
    }

    @Test("an outgoing transfer follows the peer's claims to the claimed amount")
    func outgoingFollowsPeerClaims() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.outgoing, totalValue: 10)
        let coin = try makeCoin(exponent: 2)
        let statuses = MockCoinageTransferStatusService(snapshots: [
            [:],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .awaitingClaim(finalized: false))],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .awaitingClaim(finalized: true))],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .claimed(finalized: true))]
        ])
        let states = world.stateStream(of: message.messageId)

        let monitor = makeMonitor(
            world: world,
            statuses: statuses,
            timeSource: monitorTimeSource(for: message, elapsed: insideWindow)
        )
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let claimedValue = context.valueInPlanks(for: 2)
        let seen = try await states.collect {
            $0 == .outgoing(.init(status: .claimedFinalized, actualValue: claimedValue))
        }
        let distinct = seen.enumerated().filter { $0.offset == 0 || seen[$0.offset - 1] != $0.element }.map(\.element)

        #expect(distinct == [
            nil,
            .outgoing(.init(status: .sending)),
            .outgoing(.init(status: .sent)),
            .outgoing(.init(status: .sentFinalized)),
            .outgoing(.init(status: .claimedFinalized, actualValue: claimedValue))
        ])
        #expect(seen.count == distinct.count, "every emission must carry a change")
        #expect(statuses.subscriptions.count == 1)
        #expect(statuses.reclaimGroupIds == [ChatTransferGroups.reclaim(message.messageId)])
    }

    @Test("a payment still awaiting its claim after six hours from its send time is unclaimed")
    func awaitingAfterWindowIsUnclaimed() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.outgoing, totalValue: 10)
        let statuses = try awaitingStatuses()
        let states = world.stateStream(of: message.messageId)

        let monitor = makeMonitor(
            world: world,
            statuses: statuses,
            timeSource: monitorTimeSource(for: message, elapsed: CoinageConstants.claimWindow + 1)
        )
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let seen = try await states.collect { $0 == .outgoing(.init(status: .unclaimed)) }

        #expect(seen.last == .outgoing(.init(status: .unclaimed)))
        #expect(!seen.contains(.outgoing(.init(status: .sent))))
    }

    @Test("a payment that is still awaiting its claim flips to unclaimed when the window closes")
    func windowClosingFlipsSentToUnclaimed() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.outgoing, totalValue: 10)
        let statuses = try awaitingStatuses()
        let clock = ManualClock()
        let dateProvider = MockDateProvider(
            now: Date.fromChatTimestamp(message.timestamp).addingTimeInterval(insideWindow)
        )

        let monitor = makeMonitor(
            world: world,
            statuses: statuses,
            timeSource: TimeSource(clock: clock, dateProvider: dateProvider)
        )
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        _ = try await world.stateStream(of: message.messageId)
            .collect { $0 == .outgoing(.init(status: .sent)) }
        await dateProvider.advance(by: CoinageConstants.claimWindow - insideWindow)
        await clock.resumeSleep(for: .seconds(CoinageConstants.claimWindow - insideWindow))

        let seen = try await world.stateStream(of: message.messageId)
            .collect { $0 == .outgoing(.init(status: .unclaimed)) }
        #expect(seen.last == .outgoing(.init(status: .unclaimed)))
    }

    @Test("a claim that gave up after the window is returned to the sender")
    func notClaimedAfterWindowIsReturned() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming, totalValue: 10)
        let claims = MockClaimCoinsService(detections: [.notClaimed])
        let returned = IncomingTransferState(status: .returnedToSender)

        let monitor = makeMonitor(
            world: world,
            claims: claims,
            timeSource: monitorTimeSource(for: message, elapsed: CoinageConstants.claimWindow + 1)
        )
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let seen = try await world.stateStream(of: message.messageId).collect { $0 == .incoming(returned) }
        #expect(seen.last == .incoming(returned))
    }

    @Test("coins absent inside the window keep the row detecting and flip it to returned when the window closes")
    func absentCoinsFlipToReturnedAtWindowClose() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming, totalValue: 10)
        let claims = MockClaimCoinsService(detections: [.noCoinsOnChain])
        let clock = ManualClock()
        let dateProvider = MockDateProvider(
            now: Date.fromChatTimestamp(message.timestamp).addingTimeInterval(insideWindow)
        )

        let monitor = makeMonitor(
            world: world,
            claims: claims,
            timeSource: TimeSource(clock: clock, dateProvider: dateProvider)
        )
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let remaining = CoinageConstants.claimWindow - insideWindow
        await clock.waitForSleep(for: .seconds(remaining))
        let beforeClose = try await world.stateStream(of: message.messageId)
            .collect { $0 == .incoming(.init(status: .detecting)) }
        #expect(!beforeClose.contains(.incoming(.init(status: .returnedToSender))))
        await dateProvider.advance(by: remaining)
        await clock.resumeSleep(for: .seconds(remaining))

        let returned = IncomingTransferState(status: .returnedToSender)
        let seen = try await world.stateStream(of: message.messageId).collect { $0 == .incoming(returned) }
        #expect(seen.last == .incoming(returned))
    }

    @Test("detecting and returned rows re-enter the incoming candidates while a failed one does not")
    func detectingAndReturnedRowsAreReprocessedFailedIsNot() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let detecting = try await world.saveTransfer(.incoming)
        try await world.store.updateIncoming(
            messageId: detecting.messageId,
            state: .init(status: .detecting)
        )
        let returned = try await world.saveTransfer(.incoming)
        try await world.store.updateIncoming(
            messageId: returned.messageId,
            state: .init(status: .returnedToSender)
        )
        let failed = try await world.saveTransfer(.incoming)
        try await world.store.updateIncoming(
            messageId: failed.messageId,
            state: .init(status: .failed)
        )

        let candidates = world.messageProviderFactory.subscribeMessages(with: .incomingCoinageSendMessages())
        for try await messages in candidates {
            #expect(Set(messages.map(\.messageId)) == [detecting.messageId, returned.messageId])
            break
        }
    }

    @Test("an outgoing payment follows its reclaim from failure to finality, one row per distinct state")
    func outgoingFollowsReclaimToFinality() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.outgoing, totalValue: 10)
        let coin = try makeCoin(exponent: 2)
        let statuses = MockCoinageTransferStatusService(snapshots: [
            [coin.publicKey: CoinageTransferState(coin: coin, status: .reclaimFailed)],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .awaitingClaim(finalized: false))],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .reclaiming)],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .reclaimed(finalized: false))],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .reclaimed(finalized: true))]
        ])
        let states = world.stateStream(of: message.messageId)

        let monitor = makeMonitor(
            world: world,
            statuses: statuses,
            timeSource: monitorTimeSource(for: message, elapsed: CoinageConstants.claimWindow + 1)
        )
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let value = context.valueInPlanks(for: 2)
        let seen = try await states.collect {
            $0 == .outgoing(.init(status: .reclaimedFinalized, actualValue: value))
        }
        let distinct = seen.enumerated().filter { $0.offset == 0 || seen[$0.offset - 1] != $0.element }.map(\.element)

        #expect(distinct == [
            nil,
            .outgoing(.init(status: .reclaimFailed)),
            .outgoing(.init(status: .unclaimed)),
            .outgoing(.init(status: .reclaiming)),
            .outgoing(.init(status: .reclaimed, actualValue: value)),
            .outgoing(.init(status: .reclaimedFinalized, actualValue: value))
        ])
        #expect(seen.count == distinct.count, "every emission must carry a change")
    }

    @Test("every non-final reclaim row re-enters the outgoing candidates, a finalized reclaim does not")
    func reclaimRowsReEnterCandidates() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let unclaimed = try await world.saveTransfer(.outgoing)
        try await world.store.updateOutgoing(messageId: unclaimed.messageId, state: .init(status: .unclaimed))
        let reclaiming = try await world.saveTransfer(.outgoing)
        try await world.store.updateOutgoing(messageId: reclaiming.messageId, state: .init(status: .reclaiming))
        let failed = try await world.saveTransfer(.outgoing)
        try await world.store.updateOutgoing(messageId: failed.messageId, state: .init(status: .reclaimFailed))
        let bestHead = try await world.saveTransfer(.outgoing)
        try await world.store.updateOutgoing(
            messageId: bestHead.messageId,
            state: .init(status: .reclaimed, actualValue: 4)
        )
        let finished = try await world.saveTransfer(.outgoing)
        try await world.store.updateOutgoing(
            messageId: finished.messageId,
            state: .init(status: .reclaimedFinalized, actualValue: 4)
        )

        let candidates = world.messageProviderFactory.subscribeMessages(with: .outgoingLocalDeviceCoinageSendMessages())
        for try await messages in candidates {
            #expect(Set(messages.map(\.messageId)) == [
                unclaimed.messageId, reclaiming.messageId, failed.messageId, bestHead.messageId
            ])
            break
        }
    }
}

private extension CoinageTransferMonitorTests {
    /// Half the window: inside it whatever the window's length.
    var insideWindow: TimeInterval { CoinageConstants.claimWindow / 2 }

    /// Messages are saved at the wall-clock time, so a provider frozen now sits inside every window and a
    /// manual clock never closes one; tests that need the window to close build their own source.
    func makeMonitor(
        world: TransferStateTestWorld,
        claims: MockClaimCoinsService = MockClaimCoinsService(detections: []),
        statuses: MockCoinageTransferStatusService = MockCoinageTransferStatusService(snapshots: []),
        store: (any TransferStateStoring)? = nil,
        timeSource: TimeSource = TimeSource(clock: ManualClock(), dateProvider: MockDateProvider(now: Date())),
        logger: MockLogger = MockLogger()
    ) -> CoinageTransferMonitor {
        CoinageTransferMonitor(
            claimCoinsService: claims,
            transferStatusService: statuses,
            denominationContext: { [context] in context },
            transferStateStore: store ?? world.store,
            storageFacade: world.facade,
            operationQueue: OperationQueue(),
            timeSource: timeSource,
            logger: logger
        )
    }

    /// The window is anchored to the message's send timestamp, so the provider's clock is placed relative to it.
    func monitorTimeSource(for message: Chat.LocalMessage, elapsed: TimeInterval) -> TimeSource {
        let sentAt = Date.fromChatTimestamp(message.timestamp)
        return TimeSource(
            clock: ManualClock(),
            dateProvider: MockDateProvider(now: sentAt.addingTimeInterval(elapsed))
        )
    }

    func awaitingStatuses() throws -> MockCoinageTransferStatusService {
        let coin = try makeCoin(exponent: 2)
        return MockCoinageTransferStatusService(snapshots: [
            [coin.publicKey: CoinageTransferState(coin: coin, status: .awaitingClaim(finalized: false))]
        ])
    }

    func makeCoin(exponent: Int16) throws -> Coin {
        let installation = try CoinageInstallationId(value: Data(repeating: 0x01, count: 32))
        return Coin(
            exponent: exponent,
            derivationIndex: CoinageKeyIndex(installation: installation, item: 0),
            age: 0,
            publicKey: Data(repeating: 0x09, count: 32)
        )
    }
}

private enum MonitorTestError: Error {
    case chainUnavailable
}
