import BigInt
import Coinage
import Foundation
import SubstrateSdk
import Testing
@testable import polkadot_app

/// The monitor writes exactly what these projections say, so every branch is pinned here: a wrong
/// row would show the wrong bubble and, once terminal, stop the monitor from ever looking again.
@Suite("Transfer state mapping")
struct TransferStateMappingTests {
    @Test(
        "receiver detections project onto the incoming state",
        arguments: [
            (CoinageTransferDetection.detecting, IncomingTransferState(status: .detecting)),
            (.claiming, IncomingTransferState(status: .claiming)),
            (.claimingRest(claimed: 4), IncomingTransferState(status: .claiming)),
            (.claimed(amount: 10, finalized: false), IncomingTransferState(status: .claimed, actualValue: 10)),
            (.claimed(amount: 10, finalized: true), IncomingTransferState(status: .claimed, actualValue: 10)),
            (.claimedPartially(claimed: 6), IncomingTransferState(status: .claimed, actualValue: 6)),
            (.notClaimed, IncomingTransferState(status: .failed))
        ]
    )
    func incomingMapping(detection: CoinageTransferDetection, expected: IncomingTransferState) {
        #expect(detection.incomingState == expected)
    }

    @Test("no coin states yet means sending")
    func emptyIsSending() {
        #expect([CoinagePaymentState]().outgoingState == OutgoingTransferState(status: .sending))
    }

    @Test("coins still being detected with nothing awaiting or claimed means sending")
    func onlyDetectingIsSending() {
        let states = CoinStateWorld.states([.detecting, .detecting])
        #expect(states.outgoingState == OutgoingTransferState(status: .sending))
    }

    @Test("a coin on chain waiting for the peer means sent")
    func awaitingIsSent() {
        let states = CoinStateWorld.states([.awaitingClaim, .detecting])
        #expect(states.outgoingState == OutgoingTransferState(status: .sent))
    }

    @Test("a claimed coin next to one still outstanding keeps the message at sent")
    func claimedWithOutstandingIsSent() {
        let states = CoinStateWorld.states([.claimed(finalized: true), .awaitingClaim])
        #expect(states.outgoingState == OutgoingTransferState(status: .sent))
    }

    @Test("nothing outstanding and nothing claimed means failed")
    func allFailedIsFailed() {
        let states = CoinStateWorld.states([.failed, .failed])
        #expect(states.outgoingState == OutgoingTransferState(status: .failed))
    }

    @Test("nothing outstanding sums the claimed coins into the actual value")
    func claimedSumsValue() {
        let states = CoinStateWorld.states([.claimed(finalized: true), .failed, .claimed(finalized: false)])

        let expectedValue = CoinStateWorld.value(ofExponent: 0) + CoinStateWorld.value(ofExponent: 2)
        #expect(
            states.outgoingState
                == OutgoingTransferState(status: .claimed, actualValue: expectedValue)
        )
    }

    @Test("settled only when every coin is terminal")
    func settledNeedsEveryCoinTerminal() {
        #expect(CoinStateWorld.states([.claimed(finalized: true), .failed]).isSettled)
        #expect(!CoinStateWorld.states([.claimed(finalized: false)]).isSettled)
        #expect(!CoinStateWorld.states([.claimed(finalized: true), .awaitingClaim]).isSettled)
        #expect(![CoinagePaymentState]().isSettled)
    }

    @Test("claimed and failed are the terminal statuses on both sides")
    func terminalStatuses() {
        #expect(IncomingTransferState.Status.claimed.isTerminal)
        #expect(IncomingTransferState.Status.failed.isTerminal)
        #expect(!IncomingTransferState.Status.detecting.isTerminal)
        #expect(!IncomingTransferState.Status.claiming.isTerminal)
        #expect(OutgoingTransferState.Status.claimed.isTerminal)
        #expect(OutgoingTransferState.Status.failed.isTerminal)
        #expect(!OutgoingTransferState.Status.sending.isTerminal)
        #expect(!OutgoingTransferState.Status.sent.isTerminal)
    }
}

/// Coins with exponents 0, 1, 2… in the order given, each with its own account.
private enum CoinStateWorld {
    static func value(ofExponent exponent: Int) -> BigUInt {
        BigUInt(1_000) << exponent
    }

    static func states(_ statuses: [CoinageTransferStatus]) -> [CoinagePaymentState] {
        statuses.enumerated().map { offset, status in
            CoinagePaymentState(
                account: Data(repeating: UInt8(offset + 1), count: 32),
                value: value(ofExponent: offset),
                status: status
            )
        }
    }
}
