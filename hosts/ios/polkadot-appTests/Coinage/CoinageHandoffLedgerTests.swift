import Coinage
import CoreData
import DurableTransactions
import Foundation
import Testing

@testable import polkadot_app

/// The handoff guard as the CoreData ledger enforces it, inside the write transaction.
///
/// `CoinageTxService` runs its own `filterHandedOff` check before calling the ledger, and that check is
/// enough to satisfy a suite running against the in-memory double. It is not enough in production: the
/// service reads and the ledger writes, and only the ledger's guard sits inside the transaction that
/// makes the mark. These cover the ledger directly, against a real store.
@Suite("Coinage handoff ledger")
struct CoinageHandoffLedgerTests {
    @Test("a coin already marked cannot be marked again")
    func remarkingAHandedOffCoinIsRefused() async throws {
        let world = try await CoinageLedgerWorld()
        let coin = world.coin(1)

        try await world.precommit([coin])

        await #expect(throws: CoinageTxError.handoffOfHandedOffAsset(coin.publicKey.toHex())) {
            try await world.precommit([coin])
        }
    }

    /// A committed mark is the stronger case: `releaseUncommittedHandoffs` deliberately leaves it in
    /// place, so the coin stays unavailable across a relaunch and the guard has to keep refusing it.
    @Test("a coin whose handoff was committed cannot be marked again")
    func remarkingACommittedCoinIsRefused() async throws {
        let world = try await CoinageLedgerWorld()
        let coin = world.coin(1)

        try await world.precommit([coin])
        try await world.messages.save(world.message()) { scope in
            try world.ledger.ledger.commitHandoffs([coin.publicKey], in: scope)
        }
        try await world.ledger.ledger.releaseUncommittedHandoffs()

        await #expect(throws: CoinageTxError.handoffOfHandedOffAsset(coin.publicKey.toHex())) {
            try await world.precommit([coin])
        }
    }

    /// The ledger resolves the row by derivation index, so an asset with no persisted coin has nothing
    /// to mark. Refused rather than passed over: a handoff that marked nothing would hand out keys the
    /// wallet still believes are free.
    @Test("an asset with no persisted coin is refused")
    func markingAnUnknownCoinIsRefused() async throws {
        let world = try await CoinageLedgerWorld()
        // The harness persists coins 1...3 only.
        let unknown = world.coin(9)

        await #expect(throws: CoinageTxError.handoffOfUnknownAsset(unknown.publicKey.toHex())) {
            try await world.precommit([unknown])
        }
    }
}
