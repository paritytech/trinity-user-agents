import Coinage
import Foundation
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

        let seen = try await states.collect { $0 == .incoming(.init(status: .claimed, actualValue: 10)) }
        let distinct = seen.enumerated().filter { $0.offset == 0 || seen[$0.offset - 1] != $0.element }.map(\.element)

        #expect(distinct == [
            nil,
            .incoming(.init(status: .detecting)),
            .incoming(.init(status: .claiming)),
            .incoming(.init(status: .claimed, actualValue: 10))
        ])
        #expect(seen.count == distinct.count, "every emission must carry a change")
        #expect(claims.calls.count == 1)
        #expect(claims.calls.first?.groupId == message.messageId)
    }

    @Test("a terminal row keeps the message out of the monitor on the next launch")
    func terminalRowIsNeverReprocessed() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)
        try await world.store.updateIncoming(
            messageId: message.messageId,
            state: .init(status: .claimed, actualValue: 10)
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
            [coin.publicKey: CoinageTransferState(coin: coin, status: .awaitingClaim)],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .awaitingClaim)],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .claimed(finalized: true))]
        ])
        let states = world.stateStream(of: message.messageId)

        let monitor = makeMonitor(world: world, statuses: statuses)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let claimedValue = context.valueInPlanks(for: 2)
        let seen = try await states.collect { $0 == .outgoing(.init(status: .claimed, actualValue: claimedValue)) }
        let distinct = seen.enumerated().filter { $0.offset == 0 || seen[$0.offset - 1] != $0.element }.map(\.element)

        #expect(distinct == [
            nil,
            .outgoing(.init(status: .sending)),
            .outgoing(.init(status: .sent)),
            .outgoing(.init(status: .claimed, actualValue: claimedValue))
        ])
        #expect(seen.count == distinct.count, "every emission must carry a change")
        #expect(statuses.subscriptions.count == 1)
    }
}

private extension CoinageTransferMonitorTests {
    func makeMonitor(
        world: TransferStateTestWorld,
        claims: MockClaimCoinsService = MockClaimCoinsService(detections: []),
        statuses: MockCoinageTransferStatusService = MockCoinageTransferStatusService(snapshots: []),
        logger: MockLogger = MockLogger()
    ) -> CoinageTransferMonitor {
        CoinageTransferMonitor(
            claimCoinsService: claims,
            transferStatusService: statuses,
            denominationContext: { [context] in context },
            transferStateStore: world.store,
            storageFacade: world.facade,
            operationQueue: OperationQueue(),
            logger: logger
        )
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
