import Foundation
import Testing
@testable import Coinage

/// The Appendix-A derived payment-status ladder (`CoinageTransferStatusService.transferStatus`).
/// Pure logic: a handed-off coin's status is read from its minter status and its presence at the finalized head.
@Suite("Coinage payment status (Appendix A)")
struct CoinageTransferStatusTests {
    private let key = Data(repeating: 7, count: 32)

    @Test("Finalized mint, absent at finality, is a terminal claim")
    func claimedFinalized() {
        let tracked = trackedCoin(onChain: false, everOnChain: true, minter: .finalizedSuccess)
        #expect(status(tracked, atFinalized: [key: false]) == .claimed(finalized: true))
    }

    @Test("A failed mint is failed — the key controls nothing")
    func failed() {
        let tracked = trackedCoin(onChain: false, everOnChain: false, minter: .failure)
        #expect(status(tracked, atFinalized: [:]) == .failed)
    }

    @Test("Present on the best head only is awaiting claim, not yet finalized")
    func awaitingOnChain() {
        let tracked = trackedCoin(onChain: true, everOnChain: true, minter: .pending)
        #expect(status(tracked, atFinalized: [:]) == .awaitingClaim(finalized: false))
    }

    @Test("Present on both heads is awaiting claim at finality")
    func awaitingFinalized() {
        let tracked = trackedCoin(onChain: true, everOnChain: true, minter: .finalizedSuccess)
        #expect(status(tracked, atFinalized: [key: true]) == .awaitingClaim(finalized: true))
    }

    @Test("Seen on chain, now gone, minter arrived — a best-head claim")
    func claimedBestHead() {
        let tracked = trackedCoin(onChain: false, everOnChain: true, minter: .pendingSuccess)
        #expect(status(tracked, atFinalized: [:]) == .claimed(finalized: false))
    }

    @Test("Absent on best but present at finality reports awaiting, not claimed")
    func awaitingAtFinality() {
        let tracked = trackedCoin(onChain: false, everOnChain: false, minter: .pending)
        #expect(status(tracked, atFinalized: [key: true]) == .awaitingClaim(finalized: true))
    }

    @Test("Nothing observed yet is detecting")
    func detecting() {
        let tracked = trackedCoin(onChain: false, everOnChain: false, minter: .pending)
        #expect(status(tracked, atFinalized: [:]) == .detecting)
    }

    @Test("The consumer decides before any presence rule, except a failed reclaim of a gone coin", arguments: [
        ReclaimLadderCase(
            name: "a landed reclaim beats a finalized mint absent at finality",
            consumer: .pendingSuccess, onChain: false, presentAtFinality: false,
            expected: .reclaimed(finalized: false)
        ),
        ReclaimLadderCase(
            name: "a finalized reclaim is a finalized reclaim",
            consumer: .finalizedSuccess, onChain: false, presentAtFinality: false,
            expected: .reclaimed(finalized: true)
        ),
        ReclaimLadderCase(
            name: "a reclaim still being built is reclaiming",
            consumer: .pending, onChain: false, presentAtFinality: false,
            expected: .reclaiming
        ),
        ReclaimLadderCase(
            name: "a reclaim awaiting inclusion is reclaiming",
            consumer: .pendingSubmission, onChain: false, presentAtFinality: false,
            expected: .reclaiming
        ),
        ReclaimLadderCase(
            name: "a failed consumer falls through to the presence rules",
            consumer: .failure, onChain: true, minter: .pending,
            expected: .awaitingClaim(finalized: false)
        ),
        ReclaimLadderCase(
            name: "a failed reclaim of a coin still on chain is reclaimFailed",
            inFailedReclaims: true, onChain: true, presentAtFinality: true,
            expected: .reclaimFailed
        ),
        ReclaimLadderCase(
            name: "a failed reclaim of a coin that is gone reads as the peer's claim",
            inFailedReclaims: true, onChain: false, presentAtFinality: false,
            expected: .claimed(finalized: true)
        )
    ])
    func reclaimLadder(_ ladderCase: ReclaimLadderCase) {
        let tracked = trackedCoin(
            onChain: ladderCase.onChain,
            everOnChain: true,
            minter: ladderCase.minter,
            consumer: ladderCase.consumer
        )
        let atFinalized = ladderCase.presentAtFinality.map { [key: $0] } ?? [:]
        let failedReclaims: Set<PublicKey> = ladderCase.inFailedReclaims ? [key] : []

        #expect(status(tracked, failedReclaims: failedReclaims, atFinalized: atFinalized) == ladderCase.expected)
    }

    @Test("Only a finalized reclaim and a finalized claim are terminal")
    func terminalFlags() {
        #expect(CoinageTransferStatus.failed.isTerminal)
        #expect(!CoinageTransferStatus.reclaimFailed.isTerminal)
        #expect(CoinageTransferStatus.claimed(finalized: true).isTerminal)
        #expect(!CoinageTransferStatus.claimed(finalized: false).isTerminal)
        #expect(CoinageTransferStatus.reclaimed(finalized: true).isTerminal)
        #expect(!CoinageTransferStatus.reclaimed(finalized: false).isTerminal)
        #expect(!CoinageTransferStatus.reclaiming.isTerminal)
        #expect(!CoinageTransferStatus.awaitingClaim(finalized: true).isTerminal)
        #expect(!CoinageTransferStatus.detecting.isTerminal)
    }
}

extension CoinageTransferStatusTests {
    struct ReclaimLadderCase: CustomTestStringConvertible {
        let name: String
        var consumer: CoinageTxStatus?
        var inFailedReclaims = false
        let onChain: Bool
        var minter: CoinageTxStatus = .finalizedSuccess
        var presentAtFinality: Bool?
        let expected: CoinageTransferStatus

        var testDescription: String { name }
    }
}

private extension CoinageTransferStatusTests {
    func trackedCoin(
        onChain: Bool,
        everOnChain: Bool,
        minter: CoinageTxStatus,
        consumer: CoinageTxStatus? = nil
    ) -> TrackedCoin {
        let coin = Coin(
            exponent: 1,
            derivationIndex: 0,
            age: everOnChain ? 3 : nil,
            isOnchain: onChain,
            publicKey: key
        )
        return TrackedCoin(
            coin: coin,
            state: CoinageAssetState(handedOff: true, consumerStatus: consumer, minterStatus: minter)
        )
    }

    func status(
        _ tracked: TrackedCoin,
        failedReclaims: Set<PublicKey> = [],
        atFinalized: [PublicKey: Bool]
    ) -> CoinageTransferStatus {
        CoinageTransferStatusService.status(tracked, failedReclaims: failedReclaims, atFinalized: atFinalized)
    }
}
