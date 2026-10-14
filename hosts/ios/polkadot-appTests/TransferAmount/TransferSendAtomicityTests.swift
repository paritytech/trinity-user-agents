import Coinage
import CoreData
import DurableTransactions
import Foundation
import KeyDerivation
import Operation_iOS
import Testing

@testable import polkadot_app

/// The guard on the transaction-inversion design: a payment's chat message, its handoff commit and its
/// scheduled transactions all become durable in **one** CoreData transaction, or none of them do.
///
/// The ordering matters and nothing else enforces it. A message that persists without its handoff commit
/// would let the coins it just gave away be selected again; a handoff committed without the message
/// would freeze coins for a payment that never left. `performWrite` cannot nest, which is why the
/// message store opens the transaction and hands a scope to the caller rather than the other way round.
@Suite("Transfer send atomicity")
struct TransferSendAtomicityTests {
    private let messageId = "msg-atomic-1"
    private let contact = Data(repeating: 0xC0, count: 32)

    @Test("message, handoff commit and scheduled transaction all land together")
    func everythingCommitsTogether() async throws {
        let world = try await CoinageLedgerWorld()
        let handedOff = world.coin(1)
        try await world.precommit([handedOff])

        try await world.messages.save(world.message()) { scope in
            try world.ledger.ledger.commitHandoffs([handedOff.publicKey], in: scope)
            _ = try world.schedule(spending: world.coin(2), minting: world.coin(3), in: scope)
        }

        #expect(try await world.messageExists())
        #expect(try await world.scheduledEntries().count == 1)

        // Committed, not provisional: a relaunch releases uncommitted marks and must leave this one.
        try await world.ledger.ledger.releaseUncommittedHandoffs()
        #expect(try await world.ledger.ledger.getHandoffKeys().contains(handedOff.publicKey))
    }

    @Test("a scheduled transaction is durable with no attempt and holds its inputs")
    func scheduledRowIsDurableWithoutBytes() async throws {
        let world = try await CoinageLedgerWorld()
        let spent = world.coin(2)

        try await world.messages.save(world.message()) { scope in
            _ = try world.schedule(spending: spent, minting: world.coin(3), in: scope)
        }

        let entry = try #require(try await world.scheduledEntries().first)
        #expect(entry.entry.status == .pendingSubmission)
        #expect(entry.attempt == nil)
        #expect(entry.inputs == [spent.asInput])
        #expect(entry.groupId == messageId)
    }

    // MARK: - Rollback

    @Test("a throw after scheduling rolls the message back with it")
    func throwAfterSchedulingRollsEverythingBack() async throws {
        let world = try await CoinageLedgerWorld()

        await #expect(throws: SendFailure.self) {
            try await world.messages.save(world.message()) { scope in
                _ = try world.schedule(spending: world.coin(2), minting: world.coin(3), in: scope)

                // Whatever carries the keys failed after the rows were written.
                throw SendFailure()
            }
        }

        #expect(try await !world.messageExists())
        #expect(try await world.scheduledEntries().isEmpty)
    }

    @Test("a throw after committing a handoff leaves the coins free")
    func throwAfterHandoffCommitLeavesCoinsFree() async throws {
        let world = try await CoinageLedgerWorld()
        let handedOff = world.coin(1)
        try await world.precommit([handedOff])

        await #expect(throws: SendFailure.self) {
            try await world.messages.save(world.message()) { scope in
                try world.ledger.ledger.commitHandoffs([handedOff.publicKey], in: scope)

                throw SendFailure()
            }
        }

        #expect(try await !world.messageExists())

        // The commit rolled back with the message, so the mark is provisional again — and a relaunch
        // drops it, freeing coins that were never actually given away.
        try await world.ledger.ledger.releaseUncommittedHandoffs()
        #expect(try await !world.ledger.ledger.getHandoffKeys().contains(handedOff.publicKey))
    }

    @Test("a hook that throws before writing anything still rolls the message back")
    func throwBeforeWritingRollsMessageBack() async throws {
        let world = try await CoinageLedgerWorld()

        await #expect(throws: SendFailure.self) {
            try await world.messages.save(world.message()) { _ in
                throw SendFailure()
            }
        }

        #expect(try await !world.messageExists())
    }

    @Test("a failed send leaves no trace for the next attempt to trip over")
    func aFailedSendCanBeRetried() async throws {
        let world = try await CoinageLedgerWorld()

        await #expect(throws: SendFailure.self) {
            try await world.messages.save(world.message()) { scope in
                _ = try world.schedule(spending: world.coin(2), minting: world.coin(3), in: scope)
                throw SendFailure()
            }
        }

        // The same inputs are still free, so the retry registers rather than failing the Unique
        // consumer invariant.
        try await world.messages.save(world.message()) { scope in
            _ = try world.schedule(spending: world.coin(2), minting: world.coin(3), in: scope)
        }

        #expect(try await world.messageExists())
        #expect(try await world.scheduledEntries().count == 1)
    }
}

// MARK: - Support

private struct SendFailure: Error {}
