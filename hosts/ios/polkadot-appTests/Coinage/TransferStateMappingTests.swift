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
            (CoinageTransferDetection.detecting, false, IncomingTransferState(status: .detecting)),
            (.detecting, true, IncomingTransferState(status: .detecting)),
            (.noCoinsOnChain, false, IncomingTransferState(status: .detecting)),
            (.noCoinsOnChain, true, IncomingTransferState(status: .returnedToSender)),
            (.claiming, false, IncomingTransferState(status: .claiming)),
            (.claimingRest(claimed: 4), true, IncomingTransferState(status: .claiming)),
            (.claimed(amount: 10, finalized: false), false, IncomingTransferState(status: .claimed, actualValue: 10)),
            (
                .claimed(amount: 10, finalized: true),
                true,
                IncomingTransferState(status: .claimedFinalized, actualValue: 10)
            ),
            (.claimedPartially(claimed: 6), false, IncomingTransferState(status: .claimedFinalized, actualValue: 6)),
            (.notClaimed, false, IncomingTransferState(status: .failed)),
            (.notClaimed, true, IncomingTransferState(status: .returnedToSender))
        ] as [(CoinageTransferDetection, Bool, IncomingTransferState)]
    )
    func incomingMapping(detection: CoinageTransferDetection, windowClosed: Bool, expected: IncomingTransferState) {
        #expect(detection.incomingState(windowClosed: windowClosed) == expected)
    }

    @Test("no coin states yet means sending")
    func emptyIsSending() throws {
        let world = try CoinStateWorld()
        #expect(
            [:].outgoingState(context: world.context, windowClosed: false)
                == OutgoingTransferState(status: .sending)
        )
    }

    @Test("coins still being detected with nothing awaiting or claimed means sending")
    func onlyDetectingIsSending() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.detecting, .detecting])
        #expect(
            states.outgoingState(context: world.context, windowClosed: false)
                == OutgoingTransferState(status: .sending)
        )
    }

    @Test("a coin on chain waiting for the peer means sent")
    func awaitingIsSent() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.awaitingClaim(finalized: false), .detecting])
        #expect(
            states.outgoingState(context: world.context, windowClosed: false)
                == OutgoingTransferState(status: .sent)
        )
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
        let state = try world.states(statuses).outgoingState(
            context: world.context,
            windowClosed: false
        )
        #expect(state == OutgoingTransferState(status: expected))
    }

    @Test("a claimed coin next to one still outstanding keeps the message at sent")
    func claimedWithOutstandingIsSent() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.claimed(finalized: true), .awaitingClaim(finalized: false)])
        #expect(
            states.outgoingState(context: world.context, windowClosed: false)
                == OutgoingTransferState(status: .sent)
        )
    }

    @Test("the window and the reclaim statuses rank", arguments: [
        OutgoingCase(
            name: "awaiting after the window is unclaimed",
            statuses: [.awaitingClaim(finalized: false), .claimed(finalized: true)], windowClosed: true,
            status: .unclaimed
        ),
        OutgoingCase(
            name: "every coin claimed after the window is claimed, not unclaimed",
            statuses: [.claimed(finalized: true), .claimed(finalized: false)], windowClosed: true,
            status: .claimed, valuedCoins: [0, 1]
        ),
        OutgoingCase(
            name: "only detecting after the window is still sending",
            statuses: [.detecting, .detecting], windowClosed: true,
            status: .sending
        ),
        OutgoingCase(
            name: "a best-head reclaim is reclaimed with its value, not finalized",
            statuses: [.reclaimed(finalized: false)], windowClosed: true,
            status: .reclaimed, valuedCoins: [0]
        ),
        OutgoingCase(
            name: "a best-head reclaim beside a coin still awaiting after the window is unclaimed",
            statuses: [.reclaimed(finalized: false), .awaitingClaim(finalized: false)], windowClosed: true,
            status: .unclaimed
        ),
        OutgoingCase(
            name: "a reclaim final on one coin but not another is reclaimed, not finalized",
            statuses: [.reclaimed(finalized: true), .reclaimed(finalized: false)], windowClosed: false,
            status: .reclaimed, valuedCoins: [0, 1]
        ),
        OutgoingCase(
            name: "a reclaim in flight is reclaiming",
            statuses: [.reclaiming, .awaitingClaim(finalized: false)], windowClosed: true,
            status: .reclaiming
        ),
        OutgoingCase(
            name: "a final reclaim sums only the reclaimed coins",
            statuses: [.reclaimed(finalized: true), .claimed(finalized: true), .reclaimed(finalized: true)],
            windowClosed: true,
            status: .reclaimedFinalized, valuedCoins: [0, 2]
        ),
        OutgoingCase(
            name: "a failed reclaim beats unclaimed",
            statuses: [.reclaimFailed, .claimed(finalized: true)], windowClosed: true,
            status: .reclaimFailed
        ),
        OutgoingCase(
            name: "a failed reclaim inside the window is still reclaimFailed",
            statuses: [.reclaimFailed], windowClosed: false,
            status: .reclaimFailed
        ),
        OutgoingCase(
            name: "a reclaim in flight beats a failed one on another coin",
            statuses: [.reclaiming, .reclaimFailed], windowClosed: true,
            status: .reclaiming
        )
    ])
    func outgoingPrecedence(_ outgoingCase: OutgoingCase) throws {
        let world = try CoinStateWorld()
        let states = try world.states(outgoingCase.statuses)
        let value = outgoingCase.valuedCoins.isEmpty
            ? nil
            : outgoingCase.valuedCoins.reduce(Balance(0)) { $0 + world.context.valueInPlanks(for: Int16($1)) }

        #expect(
            states.outgoingState(context: world.context, windowClosed: outgoingCase.windowClosed)
                == OutgoingTransferState(status: outgoingCase.status, actualValue: value)
        )
    }

    @Test("nothing outstanding and nothing claimed means failed")
    func allFailedIsFailed() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.failed, .failed])
        #expect(
            states.outgoingState(context: world.context, windowClosed: false)
                == OutgoingTransferState(status: .failed)
        )
    }

    @Test("nothing outstanding sums the claimed coins into the actual value, claimed until every claim is final")
    func claimedSumsValue() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.claimed(finalized: true), .failed, .claimed(finalized: false)])

        let expectedValue = world.context.valueInPlanks(for: 0) + world.context.valueInPlanks(for: 2)
        #expect(
            states.outgoingState(context: world.context, windowClosed: false)
                == OutgoingTransferState(status: .claimed, actualValue: expectedValue)
        )
    }

    @Test("every claim final is claimedFinalized with the same value")
    func settledClaimIsFinalized() throws {
        let world = try CoinStateWorld()
        let states = try world.states([.claimed(finalized: true), .failed])

        #expect(
            states.outgoingState(context: world.context, windowClosed: false)
                == OutgoingTransferState(status: .claimedFinalized, actualValue: world.context.valueInPlanks(for: 0))
        )
    }

    @Test("settled only when every coin is terminal")
    func settledNeedsEveryCoinTerminal() throws {
        let world = try CoinStateWorld()
        #expect(try world.states([.claimed(finalized: true), .failed]).isSettled)
        #expect(try world.states([.reclaimed(finalized: true), .failed]).isSettled)
        #expect(try !world.states([.claimed(finalized: false)]).isSettled)
        #expect(try !world.states([.reclaimed(finalized: false)]).isSettled)
        #expect(try !world.states([.reclaiming]).isSettled)
        #expect(try !world.states([.reclaimFailed]).isSettled)
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
        #expect(!IncomingTransferState.Status.returnedToSender.isTerminal)
        #expect(!OutgoingTransferState.Status.unclaimed.isTerminal)
        #expect(OutgoingTransferState.Status.reclaimedFinalized.isTerminal)
        #expect(!OutgoingTransferState.Status.reclaimed.isTerminal)
        #expect(!OutgoingTransferState.Status.reclaiming.isTerminal)
        #expect(!OutgoingTransferState.Status.reclaimFailed.isTerminal)
    }
}

extension TransferStateMappingTests {
    struct OutgoingCase: CustomTestStringConvertible {
        let name: String
        let statuses: [CoinageTransferStatus]
        let windowClosed: Bool
        let status: OutgoingTransferState.Status
        /// Offsets of the coins whose value the state carries; empty when it carries none.
        var valuedCoins: [Int] = []

        var testDescription: String { name }
    }
}

/// Coins with exponents 0, 1, 2… in the order given, each with its own public key.
private struct CoinStateWorld {
    let context = DenominationBreakdownContext(unit: 1_000, precision: 3, maxExponent: 10, minExponent: 0)
    private let installation: CoinageInstallationId

    init() throws {
        installation = try CoinageInstallationId(value: Data(repeating: 0x01, count: 32))
    }

    func key(at offset: Int) -> PublicKey {
        Data(repeating: UInt8(offset + 1), count: 32)
    }

    func states(_ statuses: [CoinageTransferStatus]) throws -> [PublicKey: CoinageTransferState] {
        var result: [PublicKey: CoinageTransferState] = [:]
        for (offset, status) in statuses.enumerated() {
            let publicKey = key(at: offset)
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
