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

    @Test("Present on the best head is awaiting claim")
    func awaitingOnChain() {
        let tracked = trackedCoin(onChain: true, everOnChain: true, minter: .pending)
        #expect(status(tracked, atFinalized: [:]) == .awaitingClaim)
    }

    @Test("Seen on chain, now gone, minter arrived — a best-head claim")
    func claimedBestHead() {
        let tracked = trackedCoin(onChain: false, everOnChain: true, minter: .pendingSuccess)
        #expect(status(tracked, atFinalized: [:]) == .claimed(finalized: false))
    }

    @Test("Absent on best but present at finality reports awaiting, not claimed")
    func awaitingAtFinality() {
        let tracked = trackedCoin(onChain: false, everOnChain: false, minter: .pending)
        #expect(status(tracked, atFinalized: [key: true]) == .awaitingClaim)
    }

    @Test("Nothing observed yet is detecting")
    func detecting() {
        let tracked = trackedCoin(onChain: false, everOnChain: false, minter: .pending)
        #expect(status(tracked, atFinalized: [:]) == .detecting)
    }

    @Test("Only failed and finalized-claim are terminal")
    func terminalFlags() {
        #expect(CoinageTransferStatus.failed.isTerminal)
        #expect(CoinageTransferStatus.claimed(finalized: true).isTerminal)
        #expect(!CoinageTransferStatus.claimed(finalized: false).isTerminal)
        #expect(!CoinageTransferStatus.awaitingClaim.isTerminal)
        #expect(!CoinageTransferStatus.detecting.isTerminal)
    }
}

private extension CoinageTransferStatusTests {
    func trackedCoin(onChain: Bool, everOnChain: Bool, minter: CoinageTxStatus) -> TrackedCoin {
        let coin = Coin(
            exponent: 1,
            derivationIndex: 0,
            age: everOnChain ? 3 : nil,
            isOnchain: onChain,
            publicKey: key
        )
        return TrackedCoin(
            coin: coin,
            state: CoinageAssetState(handedOff: true, consumerStatus: nil, minterStatus: minter)
        )
    }

    func status(_ tracked: TrackedCoin, atFinalized: [PublicKey: Bool]) -> CoinageTransferStatus {
        CoinageTransferStatusService.transferStatus(tracked, atFinalized: atFinalized)
    }
}
