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
            (.claimed(amount: 10, finalized: true), IncomingTransferState(status: .claimedFinalized, actualValue: 10)),
            (.claimedPartially(claimed: 6), IncomingTransferState(status: .claimedFinalized, actualValue: 6)),
            (.notClaimed, IncomingTransferState(status: .failed))
        ]
    )
    func incomingMapping(detection: CoinageTransferDetection, expected: IncomingTransferState) {
        #expect(detection.incomingState == expected)
    }

    @Test("no coin states yet means sending")
    func emptyIsSending() throws {
        let world = try CoinStateWorld()
        #expect([:].outgoingState(context: world.context) == OutgoingTransferState(status: .sending))
    }

    @Test("coins still being detected with nothing awaiting or claimed means sending")
    func onlyDetectingIsSending() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.detecting, .detecting])
        #expect(states.outgoingState(context: world.context) == OutgoingTransferState(status: .sending))
    }

    @Test("a coin on chain waiting for the peer means sent")
    func awaitingIsSent() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.awaitingClaim(finalized: false), .detecting])
        #expect(states.outgoingState(context: world.context) == OutgoingTransferState(status: .sent))
    }

    @Test(
        "sent is finalized only when every coin's reading comes from the finalized chain",
        arguments: [
            (
                [CoinageTransferStatus.awaitingClaim(finalized: true), .claimed(finalized: true), .failed],
                .sentFinalized
            ),
            ([.awaitingClaim(finalized: true), .detecting], .sent),
            ([.awaitingClaim(finalized: true), .awaitingClaim(finalized: false)], .sent)
        ] as [([CoinageTransferStatus], OutgoingTransferState.Status)]
    )
    func sentFinalizedNeedsEveryReadingFinal(
        statuses: [CoinageTransferStatus],
        expected: OutgoingTransferState.Status
    ) throws {
        let world = try CoinStateWorld()
        let state = try world.states(statuses).outgoingState(context: world.context)
        #expect(state == OutgoingTransferState(status: expected))
    }

    @Test("a claimed coin next to one still outstanding keeps the message at sent")
    func claimedWithOutstandingIsSent() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.claimed(finalized: true), .awaitingClaim(finalized: false)])
        #expect(states.outgoingState(context: world.context) == OutgoingTransferState(status: .sent))
    }

    @Test("nothing outstanding and nothing claimed means failed")
    func allFailedIsFailed() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.failed, .failed])
        #expect(states.outgoingState(context: world.context) == OutgoingTransferState(status: .failed))
    }

    @Test("nothing outstanding sums the claimed coins into the actual value, claimed until every claim is final")
    func claimedSumsValue() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.claimed(finalized: true), .failed, .claimed(finalized: false)])

        let expectedValue = world.context.valueInPlanks(for: 0) + world.context.valueInPlanks(for: 2)
        #expect(
            states.outgoingState(context: world.context)
                == OutgoingTransferState(status: .claimed, actualValue: expectedValue)
        )
    }

    @Test("every claim final is claimedFinalized with the same value")
    func settledClaimIsFinalized() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.claimed(finalized: true), .failed])

        #expect(
            states.outgoingState(context: world.context)
                == OutgoingTransferState(status: .claimedFinalized, actualValue: world.context.valueInPlanks(for: 0))
        )
    }

    @Test("settled only when every coin is terminal")
    func settledNeedsEveryCoinTerminal() throws {
        let world = try CoinStateWorld()
        #expect(try world.states([.claimed(finalized: true), .failed]).isSettled)
        #expect(try !world.states([.claimed(finalized: false)]).isSettled)
        #expect(try !world.states([.claimed(finalized: true), .awaitingClaim(finalized: true)]).isSettled)
        #expect(![:].isSettled)
    }

    @Test("finalized claims and failures are the terminal statuses on both sides")
    func terminalStatuses() {
        #expect(IncomingTransferState.Status.claimedFinalized.isTerminal)
        #expect(IncomingTransferState.Status.failed.isTerminal)
        #expect(!IncomingTransferState.Status.detecting.isTerminal)
        #expect(!IncomingTransferState.Status.claiming.isTerminal)
        #expect(!IncomingTransferState.Status.claimed.isTerminal)
        #expect(OutgoingTransferState.Status.claimedFinalized.isTerminal)
        #expect(OutgoingTransferState.Status.failed.isTerminal)
        #expect(!OutgoingTransferState.Status.sending.isTerminal)
        #expect(!OutgoingTransferState.Status.sent.isTerminal)
        #expect(!OutgoingTransferState.Status.sentFinalized.isTerminal)
        #expect(!OutgoingTransferState.Status.claimed.isTerminal)
    }
}

/// Coins with exponents 0, 1, 2… in the order given, each with its own public key.
private struct CoinStateWorld {
    let context = DenominationBreakdownContext(unit: 1_000, precision: 3, maxExponent: 10, minExponent: 0)
    private let installation: CoinageInstallationId

    init() throws {
        installation = try CoinageInstallationId(value: Data(repeating: 0x01, count: 32))
    }

    func states(_ statuses: [CoinageTransferStatus]) throws -> [PublicKey: CoinageTransferState] {
        var result: [PublicKey: CoinageTransferState] = [:]
        for (offset, status) in statuses.enumerated() {
            let publicKey = Data(repeating: UInt8(offset + 1), count: 32)
            let coin = Coin(
                exponent: Int16(offset),
                derivationIndex: CoinageKeyIndex(installation: installation, item: DerivationIndex(offset)),
                age: 0,
                publicKey: publicKey
            )
            result[publicKey] = CoinageTransferState(coin: coin, status: status)
        }
        return result
    }
}
