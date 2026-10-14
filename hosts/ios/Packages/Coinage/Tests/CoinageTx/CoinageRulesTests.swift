import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Testing
@testable import Coinage

/// The engine's ladder composed with coinage's real pass scope over hand-built evidence.
///
/// `CoinageResourceOracleTests` asks the scope its two questions and `CompletionLadderTests` runs the
/// ladder over a stub scope; neither pins what the two decide together. These do, one case per rule of the
/// original evaluator, so a change to either half that moves a composed verdict is caught here. The view
/// is a stub consulted only by the body search.
@Suite("Coinage Rules")
struct CoinageRulesTests {
    // MARK: Rule 0 — recorded inclusion

    @Test("Rule 0 finalizes when the recorded block is canonical and at or below the finalized head")
    func rule0FinalizesCanonicalAtOrBelowFinalized() async throws {
        let entry = entry(outputs: [coinOut], successDetectedAt: block(120))

        let result = await evaluate(entry, evidence(finalizedNumber: 130), recordedStillCanonical: true)

        #expect(result.status == .finalizedSuccess)
    }

    @Test("Rule 0 holds at pendingSuccess while the recorded block is above the finalized head")
    func rule0HoldsPendingSuccessAboveFinalized() async throws {
        let entry = entry(outputs: [coinOut], successDetectedAt: block(140))

        let result = await evaluate(entry, evidence(finalizedNumber: 130), recordedStillCanonical: true)

        #expect(result.status == .pendingSuccess)
    }

    @Test("Rule 0 clause 1 re-records the best head when execution is still visible there")
    func rule0RecordGoneReRecordsBestHead() async throws {
        let entry = entry(outputs: [coinOut], successDetectedAt: block(120))

        let result = await evaluate(
            entry,
            evidence(finalizedNumber: 130, presentAtBest: [coinOut.publicKey]),
            recordedStillCanonical: false
        )

        #expect(result.status == .pendingSuccess)
        #expect(result.verdict?.successDetectedAt == result.view.bestHead)
    }

    @Test("Rule 0 clause 1 demotes to pending and clears the record when nothing is visible any more")
    func rule0RecordGoneDemotes() async throws {
        let entry = entry(outputs: [coinOut], successDetectedAt: block(120))

        let result = await evaluate(entry, evidence(finalizedNumber: 130), recordedStillCanonical: false)

        #expect(result.status == .pending)
        #expect(result.verdict?.successDetectedAt == nil)
    }

    @Test("Rule 0 aborts the entry when the canonicality read failed")
    func rule0UndecidedWhenCanonicalityUnread() async throws {
        let entry = entry(outputs: [coinOut], successDetectedAt: block(120))

        let result = await evaluate(entry, evidence(finalizedNumber: 130), recordedStillCanonical: nil)

        #expect(result.outcome == .undecided)
    }

    // MARK: Rules 1 and 2 — visible execution

    @Test("Rule 1 wins over Rule 2 on the same evidence")
    func rule1WinsOverRule2() async throws {
        let entry = entry(outputs: [coinOut])

        let result = await evaluate(
            entry,
            evidence(presentAtFinalized: [coinOut.publicKey], presentAtBest: [coinOut.publicKey])
        )

        #expect(result.status == .finalizedSuccess)
    }

    @Test("Rule 2 records the best head so the outputs keep optimistic selectability")
    func rule2RecordsBestHead() async throws {
        let entry = entry(outputs: [coinOut])

        let result = await evaluate(entry, evidence(presentAtBest: [coinOut.publicKey]))

        #expect(result.status == .pendingSuccess)
        #expect(result.verdict?.successDetectedAt == result.view.bestHead)
    }

    @Test("an unloaded voucher input counts as execution")
    func unloadedVoucherInputIsExecution() async throws {
        let entry = entry(inputs: [voucherIn])

        let result = await evaluate(entry, evidence(unloadedAtFinalized: [voucherIn.publicKey]))

        #expect(result.status == .finalizedSuccess)
    }

    // MARK: Rules 3 and 4 — mortality expired

    @Test("Rule 3 fails the entry when an untouched output is absent after mortality")
    func rule3FailsOnAbsentOutputAfterMortality() async throws {
        let entry = entry(outputs: [coinOut])

        let result = await evaluate(
            entry,
            evidence(finalizedNumber: Self.mortalityEnd + 1, absentAtFinalized: [coinOut.publicKey])
        )

        #expect(result.status == .failure)
    }

    @Test("Rule 3 does not fire before mortality has expired")
    func rule3DoesNotFireBeforeMortality() async throws {
        let entry = entry(outputs: [coinOut])

        let result = await evaluate(
            entry,
            evidence(
                finalizedNumber: Self.mortalityEnd,
                absentAtFinalized: [coinOut.publicKey],
                absentAtBest: [coinOut.publicKey]
            )
        )

        #expect(result.status == .pending)
        #expect(!result.reachedSearch)
    }

    @Test("Rule 4 fails the entry when an input is still available after mortality")
    func rule4FailsOnAvailableInputAfterMortality() async throws {
        let entry = entry(inputs: [coinIn], outputs: [coinOut])

        let result = await evaluate(
            entry,
            evidence(
                finalizedNumber: Self.mortalityEnd + 1,
                presentAtFinalized: [coinIn.publicKey],
                // Not absent, so the output clause cannot fire and the input clause is reached.
                unreadable: [coinOut.publicKey]
            )
        )

        #expect(result.status == .failure)
    }

    @Test("Rule 4 does not fire before mortality has expired")
    func rule4DoesNotFireBeforeMortality() async throws {
        let entry = entry(inputs: [coinIn], outputs: [coinOut])

        let result = await evaluate(
            entry,
            evidence(
                finalizedNumber: Self.mortalityEnd,
                presentAtFinalized: [coinIn.publicKey],
                presentAtBest: [coinIn.publicKey],
                unreadable: [coinOut.publicKey]
            )
        )

        #expect(result.status == .pending)
        #expect(!result.reachedSearch)
    }

    // MARK: Rules 5 and 6 — our own coins gone

    @Test("Rule 5 finalizes when every own-coin input is gone at the finalized head")
    func rule5FinalizesWhenOwnCoinsGone() async throws {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn])

        let result = await evaluate(
            entry,
            evidence(absentAtFinalized: [coinIn.publicKey], absentAtBest: [coinIn.publicKey]),
            dag: dag(minter, entry)
        )

        #expect(result.status == .finalizedSuccess)
    }

    @Test("Rule 6 does not fire on an unexecuted entry — a registered unincluded split stays pending")
    func rule6DoesNotFireOnUnexecutedEntry() async throws {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn], outputs: [coinOut])

        // The input is still there at both heads: the split never executed.
        let result = await evaluate(
            entry,
            evidence(
                presentAtFinalized: [coinIn.publicKey],
                absentAtFinalized: [coinOut.publicKey],
                presentAtBest: [coinIn.publicKey],
                absentAtBest: [coinOut.publicKey]
            ),
            dag: dag(minter, entry)
        )

        #expect(result.status == .pending)
    }

    @Test("ownCoinInputs declines when an input has ever carried a handoff mark")
    func ownCoinInputsDeclineOnHandoff() async throws {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn])

        let result = await evaluate(
            entry,
            evidence(absentAtFinalized: [coinIn.publicKey], absentAtBest: [coinIn.publicKey]),
            dag: dag(minter, entry, handedOff: [coinIn.publicKey])
        )

        // Falls through to the search rather than reading absence as consumption.
        #expect(result.status == .pending)
        #expect(result.reachedSearch)
    }

    @Test("ownCoinInputs declines while the input minter's own window is still open")
    func ownCoinInputsDeclineWhileMinterWindowOpen() async throws {
        let minter = finalizedMinter(coinIn, checkpointNumber: 200)
        let entry = entry(inputs: [coinIn])

        let result = await evaluate(
            entry,
            evidence(absentAtFinalized: [coinIn.publicKey], absentAtBest: [coinIn.publicKey]),
            dag: dag(minter, entry)
        )

        #expect(result.status == .pending)
        #expect(result.reachedSearch)
    }

    @Test("a failed read never satisfies absent, so Rule 5 cannot fire on it")
    func failedReadNeverSatisfiesAbsent() async throws {
        let minter = finalizedMinter(coinIn)
        let entry = entry(inputs: [coinIn])

        let result = await evaluate(entry, evidence(unreadable: [coinIn.publicKey]), dag: dag(minter, entry))

        #expect(result.status == .pending)
        #expect(result.reachedSearch)
    }

    // MARK: Rule 7 — body search

    @Test("Rule 7 finalizes on a successful dispatch in the searched block")
    func rule7FinalizesOnSuccess() async throws {
        let entry = entry(inputs: [receivedIn])

        let result = await evaluate(
            entry,
            evidence(finalizedNumber: Self.mortalityEnd + 1, unreadable: [receivedIn.publicKey]),
            search: .foundSucceeded(block(120))
        )

        #expect(result.status == .finalizedSuccess)
        #expect(result.verdict?.successDetectedAt == block(120))
    }

    @Test("Rule 7 fails on a failed dispatch — inclusion is not success")
    func rule7FailsOnFailedDispatch() async throws {
        let entry = entry(inputs: [receivedIn])

        let result = await evaluate(
            entry,
            evidence(finalizedNumber: Self.mortalityEnd + 1, unreadable: [receivedIn.publicKey]),
            search: .foundFailed(block(120), reason: "Test.Failed")
        )

        #expect(result.status == .failure)
    }

    @Test("Rule 7 leaves the entry pending when the outcome could not be read")
    func rule7PendingWhenOutcomeUnreadable() async throws {
        let entry = entry(inputs: [receivedIn])

        let result = await evaluate(
            entry,
            evidence(finalizedNumber: Self.mortalityEnd + 1, unreadable: [receivedIn.publicKey]),
            search: .foundOutcomeUnreadable(block(120))
        )

        #expect(result.status == .pending)
    }

    @Test("Rule 7 fails only once the whole window was read and mortality has expired")
    func rule7FailsWhenWholeWindowReadAndExpired() async throws {
        let entry = entry(inputs: [receivedIn])

        let result = await evaluate(
            entry,
            evidence(finalizedNumber: Self.mortalityEnd + 1, unreadable: [receivedIn.publicKey]),
            search: .notFoundWindowComplete
        )

        #expect(result.status == .failure)
    }

    @Test("a partially read window leaves the entry pending")
    func partiallyReadWindowStaysPending() async throws {
        let entry = entry(inputs: [receivedIn])

        let result = await evaluate(
            entry,
            evidence(finalizedNumber: Self.mortalityEnd + 1, unreadable: [receivedIn.publicKey]),
            search: .incomplete
        )

        #expect(result.status == .pending)
    }
}

// MARK: - Harness

/// One composed evaluation: the ladder's outcome plus the view it ran against, so a case can assert on
/// the recorded head and on whether the search was reached.
private struct LadderEvaluation {
    let outcome: RuleOutcome
    let view: StubPinnedChainView
    let txHash: Data

    var verdict: Verdict? { outcome.verdict }
    var status: CoinageTxStatus? { verdict?.status }
    var reachedSearch: Bool { view.searchedHashes.contains(txHash) }
}

private extension CoinageRulesTests {
    var coinIn: CoinageTxInput { .coin(.own(1, testKey(1))) }
    var coinOut: OwnAsset { .coin(2, testKey(2)) }
    var voucherIn: CoinageTxInput { .recyclerVoucher(3, testKey(3)) }
    /// A coin whose key a peer sent us: no local identity, only an on-chain one.
    var receivedIn: CoinageTxInput { .coin(.received(Data([4]))) }

    static let checkpointNumber: UInt32 = 100
    static let mortality: UInt32 = 64
    static let mortalityEnd = checkpointNumber + mortality

    static let entryId = UUID(uuidString: "00000000-0000-0000-0000-000000000002")!
    static let minterId = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!

    func block(_ number: UInt32) -> BlockRef {
        BlockRef(number: number, hash: Data("block\(number)".utf8))
    }

    /// Runs the real ladder over the real coinage scope. The search answers `search` for this entry —
    /// `incomplete` by default, so a case about a rule above the search is not decided by it.
    func evaluate(
        _ entry: CoinageTxEntry,
        _ evidence: ChainEvidence,
        dag: CoinageEntryDag? = nil,
        search: BodySearchOutcome = .incomplete,
        recordedStillCanonical: Bool? = nil
    ) async -> LadderEvaluation {
        let view = StubPinnedChainView(finalized: evidence.finalized, best: evidence.best)
        view.setBodySearchResponse(entry.submittedAttempt.txHash, to: search)
        let scope = CoinagePassScope(dag: dag ?? self.dag(entry), evidence: [entry.id: evidence])

        let outcome = await CompletionLadder().evaluate(
            entry.entry,
            scope: scope,
            view: view,
            recordedStillCanonical: recordedStillCanonical
        )
        return LadderEvaluation(outcome: outcome, view: view, txHash: entry.submittedAttempt.txHash)
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
        id: CoinageTxId = CoinageRulesTests.entryId,
        inputs: [CoinageTxInput] = [],
        outputs: [OwnAsset] = [],
        status: CoinageTxStatus = .pending,
        successDetectedAt: BlockRef? = nil,
        checkpointNumber: UInt32 = CoinageRulesTests.checkpointNumber
    ) -> CoinageTxEntry {
        CoinageTxEntry(
            id: id,
            inputs: inputs,
            outputs: outputs,
            txHash: Data("tx\(id.uuidString)".utf8),
            checkpoint: BlockRef(number: checkpointNumber, hash: Data("checkpoint".utf8)),
            mortality: Self.mortality,
            successDetectedAt: successDetectedAt,
            status: status
        )
    }

    /// Builds `ChainEvidence` from identifier sets. An identifier in `unreadable` appears in no map — a
    /// failed read — so every predicate over it is false. The best head is always block 200.
    func evidence(
        finalizedNumber: UInt32 = 150,
        presentAtFinalized: [PublicKey] = [],
        absentAtFinalized: [PublicKey] = [],
        presentAtBest: [PublicKey] = [],
        absentAtBest: [PublicKey] = [],
        unloadedAtFinalized: [PublicKey] = [],
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
