import DurableTransactions
import Foundation
import Testing
@testable import Coinage

/// Coinage's answers to the engine's two questions. The ladder's ordering and mortality gating are the
/// engine's (`CompletionLadderTests`); what is tested here is only what is about coins and vouchers,
/// asked directly of the pass scope over hand-built evidence and a graph.
@Suite("Coinage Resource Oracle")
struct CoinageResourceOracleTests {
    // MARK: Completion

    @Test("A minted output visible at a head proves completion there")
    func outputPresentProvesCompletion() {
        let entry = entry(outputs: [coinOut])
        let scope = scope(entry, evidence(presentAtFinalized: [coinOut.publicKey], presentAtBest: [coinOut.publicKey]))

        #expect(scope.provenCompleted(entry.entry, at: .finalized))
        #expect(scope.provenCompleted(entry.entry, at: .best))
    }

    @Test("An output visible only at the best head proves completion only there")
    func outputPresentAtBestOnly() {
        let entry = entry(outputs: [coinOut])
        let scope = scope(entry, evidence(presentAtBest: [coinOut.publicKey]))

        #expect(!scope.provenCompleted(entry.entry, at: .finalized))
        #expect(scope.provenCompleted(entry.entry, at: .best))
    }

    @Test("An unloaded voucher input counts as execution")
    func unloadedVoucherInputIsExecution() {
        let entry = entry(inputs: [voucherIn])
        let scope = scope(entry, evidence(unloadedAtFinalized: [voucherIn.publicKey]))

        #expect(scope.provenCompleted(entry.entry, at: .finalized))
    }

    @Test("Every own-coin input gone at the finalized head proves completion")
    func ownCoinsGoneProvesCompletion() {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn])
        let scope = scope(
            entry,
            evidence(absentAtFinalized: [coinIn.publicKey], absentAtBest: [coinIn.publicKey]),
            dag: dag(minter, entry)
        )

        #expect(scope.provenCompleted(entry.entry, at: .finalized))
    }

    @Test("Own-coin inputs still present prove nothing — a registered unincluded split is unexecuted")
    func ownCoinsPresentProveNothing() {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn], outputs: [coinOut])
        let scope = scope(
            entry,
            evidence(
                presentAtFinalized: [coinIn.publicKey],
                absentAtFinalized: [coinOut.publicKey],
                presentAtBest: [coinIn.publicKey],
                absentAtBest: [coinOut.publicKey]
            ),
            dag: dag(minter, entry)
        )

        #expect(!scope.provenCompleted(entry.entry, at: .finalized))
        #expect(!scope.provenCompleted(entry.entry, at: .best))
    }

    @Test("Own-coin inputs decline when an input has ever carried a handoff mark")
    func ownCoinInputsDeclineOnHandoff() {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn])
        let scope = scope(
            entry,
            evidence(absentAtFinalized: [coinIn.publicKey], absentAtBest: [coinIn.publicKey]),
            dag: dag(minter, entry, handedOff: [coinIn.publicKey])
        )

        #expect(!scope.provenCompleted(entry.entry, at: .finalized))
    }

    @Test("Own-coin inputs decline while the input minter's own window is still open")
    func ownCoinInputsDeclineWhileMinterWindowOpen() {
        let minter = finalizedMinter(coinIn, checkpointNumber: 200)
        let entry = entry(inputs: [coinIn])
        let scope = scope(
            entry,
            evidence(absentAtFinalized: [coinIn.publicKey], absentAtBest: [coinIn.publicKey]),
            dag: dag(minter, entry)
        )

        #expect(!scope.provenCompleted(entry.entry, at: .finalized))
    }

    @Test("A failed read never satisfies absent")
    func failedReadNeverSatisfiesAbsent() {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn])
        let scope = scope(entry, evidence(unreadable: [coinIn.publicKey]), dag: dag(minter, entry))

        #expect(!scope.provenCompleted(entry.entry, at: .finalized))
        #expect(!scope.provenNotCompleted(entry.entry, at: .finalized))
    }

    @Test("A received-coin input gone at the best head proves nothing — only proven own coins do")
    func receivedCoinInputProvesNothing() {
        let entry = entry(inputs: [receivedIn])
        let scope = scope(
            entry,
            evidence(presentAtFinalized: [receivedIn.publicKey], absentAtBest: [receivedIn.publicKey])
        )

        #expect(!scope.provenCompleted(entry.entry, at: .best))
    }

    @Test("A finalized successor proves completion at the finalized head")
    func finalizedSuccessorProvesCompletion() {
        let minter = entry(outputs: [coinOut])
        let consumer = entry(id: Self.consumerId, inputs: [coinOut.asInput], status: .finalizedSuccess)
        let scope = scope(minter, evidence(unreadable: [coinOut.publicKey]), dag: dag(minter, consumer))

        #expect(scope.provenCompleted(minter.entry, at: .finalized))
        #expect(!scope.provenCompleted(minter.entry, at: .best))
    }

    // MARK: Non-completion

    @Test("An untouched output absent at a head proves non-completion there")
    func untouchedAbsentOutputProvesNotCompleted() {
        let entry = entry(outputs: [coinOut])
        let scope = scope(entry, evidence(absentAtFinalized: [coinOut.publicKey], absentAtBest: [coinOut.publicKey]))

        #expect(scope.provenNotCompleted(entry.entry, at: .finalized))
        #expect(scope.provenNotCompleted(entry.entry, at: .best))
    }

    @Test("A handed-off output is not read as freely gone")
    func handedOffOutputNotReadAsGone() {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn], outputs: [coinOut])
        let scope = scope(
            entry,
            evidence(
                absentAtFinalized: [coinIn.publicKey, coinOut.publicKey],
                absentAtBest: [coinIn.publicKey, coinOut.publicKey]
            ),
            dag: dag(minter, entry, handedOff: [coinOut.publicKey])
        )

        #expect(!scope.provenNotCompleted(entry.entry, at: .finalized))
        // The proven-own-coin input still completes it.
        #expect(scope.provenCompleted(entry.entry, at: .finalized))
    }

    @Test("An output a live entry still claims is not read as gone")
    func liveConsumerOutputNotReadAsGone() {
        let minter = finalizedMinter(coinIn)
        let consumer = entry(id: Self.consumerId, inputs: [coinOut.asInput])
        let entry = entry(inputs: [coinIn], outputs: [coinOut])
        let scope = scope(
            entry,
            evidence(
                absentAtFinalized: [coinIn.publicKey, coinOut.publicKey],
                absentAtBest: [coinIn.publicKey, coinOut.publicKey]
            ),
            dag: dag(minter, entry, consumer)
        )

        #expect(!scope.provenNotCompleted(entry.entry, at: .finalized))
        #expect(scope.provenCompleted(entry.entry, at: .finalized))
    }

    @Test("An input still available at a head proves non-completion there")
    func availableInputProvesNotCompleted() {
        let entry = entry(inputs: [coinIn], outputs: [coinOut])
        let scope = scope(
            entry,
            evidence(
                presentAtFinalized: [coinIn.publicKey],
                presentAtBest: [coinIn.publicKey],
                unreadable: [coinOut.publicKey]
            )
        )

        #expect(scope.provenNotCompleted(entry.entry, at: .finalized))
        #expect(scope.provenNotCompleted(entry.entry, at: .best))
    }

    @Test("A voucher input is available only while present and not unloaded")
    func voucherAvailableOnlyWhenNotUnloaded() {
        let entry = entry(inputs: [voucherIn])
        let present = scope(
            entry,
            evidence(presentAtFinalized: [voucherIn.publicKey], notUnloadedAtFinalized: [voucherIn.publicKey])
        )
        let unknownAlias = scope(entry, evidence(presentAtFinalized: [voucherIn.publicKey]))

        #expect(present.provenNotCompleted(entry.entry, at: .finalized))
        #expect(!unknownAlias.provenNotCompleted(entry.entry, at: .finalized))
    }

    @Test("An entry the snapshot does not hold answers nothing")
    func unknownEntryAnswersNothing() {
        let entry = entry(outputs: [coinOut])
        let scope = CoinagePassScope(dag: CoinageEntryDag(entries: [], handedOff: []), evidence: [:])

        #expect(!scope.provenCompleted(entry.entry, at: .finalized))
        #expect(!scope.provenNotCompleted(entry.entry, at: .finalized))
    }
}

// MARK: - Harness

private extension CoinageResourceOracleTests {
    var coinIn: CoinageTxInput { .coin(.own(1, testKey(1))) }
    var coinOut: OwnAsset { .coin(2, testKey(2)) }
    var voucherIn: CoinageTxInput { .recyclerVoucher(3, testKey(3)) }
    var receivedIn: CoinageTxInput { .coin(.received(Data([4]))) }

    static let checkpointNumber: UInt32 = 100
    static let mortality: UInt32 = 64

    func block(_ number: UInt32) -> BlockRef {
        BlockRef(number: number, hash: Data("block\(number)".utf8))
    }

    func scope(_ entry: CoinageTxEntry, _ evidence: ChainEvidence, dag: CoinageEntryDag? = nil) -> CoinagePassScope {
        CoinagePassScope(dag: dag ?? self.dag(entry), evidence: [entry.id: evidence])
    }

    func dag(_ entries: CoinageTxEntry..., handedOff: Set<PublicKey> = []) -> CoinageEntryDag {
        CoinageEntryDag(entries: entries, handedOff: handedOff)
    }

    /// A finalized entry that minted `input`'s asset long enough ago that its window has closed.
    func finalizedMinter(_ input: CoinageTxInput, checkpointNumber: UInt32 = 0) -> CoinageTxEntry {
        entry(
            id: Self.minterId,
            outputs: [input.ownAsset ?? .coin(0, testKey(0))],
            status: .finalizedSuccess,
            checkpointNumber: checkpointNumber
        )
    }

    func entry(
        id: CoinageTxId = CoinageResourceOracleTests.entryId,
        inputs: [CoinageTxInput] = [],
        outputs: [OwnAsset] = [],
        status: CoinageTxStatus = .pending,
        checkpointNumber: UInt32 = CoinageResourceOracleTests.checkpointNumber
    ) -> CoinageTxEntry {
        CoinageTxEntry(
            id: id,
            inputs: inputs,
            outputs: outputs,
            txHash: Data("tx\(id.uuidString)".utf8),
            checkpoint: BlockRef(number: checkpointNumber, hash: Data("checkpoint".utf8)),
            mortality: Self.mortality,
            status: status
        )
    }

    static let entryId = UUID(uuidString: "00000000-0000-0000-0000-000000000002")!
    static let minterId = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!
    static let consumerId = UUID(uuidString: "00000000-0000-0000-0000-000000000003")!

    /// Builds `ChainEvidence` from identifier sets. An identifier in `unreadable` appears in no map — a
    /// failed read — so every predicate over it is false.
    func evidence(
        finalizedNumber: UInt32 = 150,
        presentAtFinalized: [PublicKey] = [],
        absentAtFinalized: [PublicKey] = [],
        presentAtBest: [PublicKey] = [],
        absentAtBest: [PublicKey] = [],
        unloadedAtFinalized: [PublicKey] = [],
        notUnloadedAtFinalized: [PublicKey] = [],
        unreadable: [PublicKey] = []
    ) -> ChainEvidence {
        func presence(_ present: [PublicKey], _ absent: [PublicKey]) -> [PublicKey: ChainPresence] {
            var result: [PublicKey: ChainPresence] = [:]
            for key in present where !unreadable.contains(key) {
                result[key] = .present
            }
            for key in absent where !unreadable.contains(key) {
                result[key] = .absent
            }
            return result
        }

        var alias: [PublicKey: AliasRead] = [:]
        for key in unloadedAtFinalized {
            alias[key] = .unloaded
        }
        for key in notUnloadedAtFinalized {
            alias[key] = .notUnloaded
        }

        return ChainEvidence(
            finalized: block(finalizedNumber),
            best: block(200),
            presenceAtFinalized: presence(presentAtFinalized, absentAtFinalized),
            presenceAtBest: presence(presentAtBest, absentAtBest),
            aliasAtFinalized: alias,
            aliasAtBest: alias
        )
    }
}
