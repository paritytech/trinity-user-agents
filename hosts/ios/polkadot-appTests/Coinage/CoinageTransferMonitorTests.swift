import Coinage
import Foundation
import SubstrateSdk
import Testing
@testable import polkadot_app

/// The monitor, the store and the chat snapshot together: a scripted claim must reach the message as
/// one emission per distinct state, a terminal row must keep the monitor away for good, and a runtime
/// failure must leave the row exactly as the wallet last reported it.
@Suite("Coinage transfer monitor", .serialized)
struct CoinageTransferMonitorTests {
    @Test("an incoming claim reaches the message once per distinct state")
    func incomingClaimReachesMessage() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming, totalValue: 10)
        let wallet = MockCoinageService(detections: [
            .detecting, .detecting, .claiming, .claimingRest(claimed: 4),
            .claimed(amount: 10, finalized: false), .claimed(amount: 10, finalized: true)
        ])
        let states = world.stateStream(of: message.messageId)

        let monitor = makeMonitor(world: world, wallet: wallet)
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
        #expect(wallet.claimCalls.count == 1)
        #expect(try wallet.claimCalls.first?.operationId == .chatClaim(messageId: message.messageId))
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
        let wallet = MockCoinageService(detections: [.claimed(amount: 10, finalized: true)])

        let monitor = makeMonitor(world: world, wallet: wallet)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let candidates = world.messageProviderFactory.subscribeMessages(with: .incomingCoinageSendMessages())
        for try await messages in candidates {
            #expect(messages.isEmpty)
            break
        }
        #expect(wallet.claimCalls.isEmpty)
    }

    /// The failure log is the last statement of the monitor's catch, so once it has been written the
    /// task can no longer touch the row: what the row holds then is what it will hold.
    @Test("a runtime failure leaves the last state the wallet reported in place")
    func runtimeFailureDoesNotWriteStatus() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)
        let wallet = MockCoinageService(detections: [.claiming], claimFailure: MonitorTestError.chainUnavailable)
        let logger = MockLogger()

        let monitor = makeMonitor(world: world, wallet: wallet, logger: logger)
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
        let claimedValue: Balance = 4_000
        let account = Data(repeating: 0x09, count: 32)
        let wallet = MockCoinageService(paymentSnapshots: [
            [],
            [CoinagePaymentState(account: account, value: claimedValue, status: .awaitingClaim)],
            [CoinagePaymentState(account: account, value: claimedValue, status: .awaitingClaim)],
            [CoinagePaymentState(account: account, value: claimedValue, status: .claimed(finalized: true))]
        ])
        let states = world.stateStream(of: message.messageId)

        let monitor = makeMonitor(world: world, wallet: wallet)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        let seen = try await states.collect { $0 == .outgoing(.init(status: .claimed, actualValue: claimedValue)) }
        let distinct = seen.enumerated().filter { $0.offset == 0 || seen[$0.offset - 1] != $0.element }.map(\.element)

        #expect(distinct == [
            nil,
            .outgoing(.init(status: .sending)),
            .outgoing(.init(status: .sent)),
            .outgoing(.init(status: .claimed, actualValue: claimedValue))
        ])
        #expect(seen.count == distinct.count, "every emission must carry a change")
        #expect(try wallet.paymentSubscriptions == [.chatSend(messageId: message.messageId)])
    }
}

private extension CoinageTransferMonitorTests {
    func makeMonitor(
        world: TransferStateTestWorld,
        wallet: MockCoinageService = MockCoinageService(),
        logger: MockLogger = MockLogger()
    ) -> CoinageTransferMonitor {
        CoinageTransferMonitor(
            coinageService: wallet,
            transferStateStore: world.store,
            storageFacade: world.facade,
            operationQueue: OperationQueue(),
            logger: logger
        )
    }
}

private enum MonitorTestError: Error {
    case chainUnavailable
}
