package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CheckpointBlock
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.AssetPublicKey
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerEntry
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import kotlin.random.Random

/**
 * Random walks over coins and reorgs, checked against invariants rather than expected verdicts.
 *
 * A walk has no expected outcome, so the oracle is what the chain itself says: the fake chain behind the
 * harness is ground truth, and every claim the ledger makes is checked against it. `PENDING` is always
 * allowed — this catches a verdict that is confident and wrong, never one that is merely undecided.
 *
 * Actions are drawn from [enabledActions], which only ever offers transitions the runtime could produce.
 * The two that matter: an extrinsic is never applied outside its mortality era, and a failed dispatch
 * leaves untouched everything this subsystem reads, because `extension.rs` restores the coin in
 * `post_dispatch` and the aliases an unload would have written roll back with the dispatch. The
 * `LockedCoins` row a failure leaves behind is a map the spec puts out of scope.
 */
class CoinageFuzzDriver(private val harness: DurabilityHarness) {
    private var nextCoin = FIRST_OUTPUT_COIN

    /** Addresses are never reused, so a voucher index is offered for minting at most once. */
    private val mintedKeys = mutableSetOf<Any>()

    /**
     * Clean passes each entry has seen since its window closed.
     *
     * Counted rather than requiring two passes back to back: passes are one action among many, so
     * consecutive ones are rare and a check that waited for them would almost never apply. What the property
     * needs is that the entry has been looked at twice since it expired, not that nothing happened in
     * between — the chain moving on cannot make an expired entry decidable again.
     */
    private val expiredPasses = mutableMapOf<Long, Int>()

    /** Clean passes each entry has seen since its transaction succeeded in a block below the finalized head. */
    private val settledPasses = mutableMapOf<Long, Int>()

    /** Marks whose keys really did leave, so a relaunch must keep them however it clears uncommitted ones. */
    private val committedMarks = mutableSetOf<AssetPublicKey>()
    private val terminalSeen = mutableMapOf<Long, Pair<DurableTxStatus, CheckpointBlock?>>()

    /** See [canonicalTransactions]. [indexedHash] is the block [indexedUpTo] named when it was folded in. */
    private val canonicalIndex = mutableMapOf<String, CanonicalInclusion>()
    private var indexedUpTo = -1L
    private var indexedHash: String? = null

    suspend fun walk(random: Random, steps: Int, profile: FuzzProfile): List<FuzzAction> {
        val taken = mutableListOf<FuzzAction>()

        repeat(steps) {
            val action = profile.pick(random, enabledActions()) ?: return@repeat
            apply(action)
            taken += action
            checkInvariants(taken)
        }

        return taken
    }

    /** Replays a recorded walk, so a failure can be shrunk and re-run deterministically. */

    /**
     * Replays a recorded walk exactly, or gives up.
     *
     * An action that is not enabled means this candidate is no longer the walk that was recorded: entry ids
     * and coin indices are handed out in order, so dropping a registration renumbers everything after it and
     * the remaining actions address different entities. Skipping such an action and carrying on — which this
     * used to do — makes the shrinker judge a different history from the one it is trying to reduce, so it
     * keeps whatever it happened to land on and reports a trace that does not reproduce.
     */
    suspend fun replay(actions: List<FuzzAction>) {
        actions.forEachIndexed { index, action ->
            if (action !in enabledActions()) throw ReplayDiverged
            apply(action)
            checkInvariants(actions.take(index + 1))
        }
    }

    private suspend fun enabledActions(): List<FuzzAction> = buildList {
        add(FuzzAction.ProduceBlock)
        add(FuzzAction.FinalizeToBest)
        add(FuzzAction.RunPass)
        FuzzFault.entries.forEach { add(FuzzAction.RunPassWithFault(it)) }

        // A reorg can never cross the finalized head.
        harness.chain.chain.reorgDepths.forEach { add(FuzzAction.Reorg(it)) }

        val entries = harness.repository.getAllEntries().getOrThrow()
        val claimed = entries.filter { it.status != FAILURE }.flatMap { entry -> entry.inputs.map { it.publicKey } }.toSet()

        // Read once: what the chain holds does not change while the enabled set is being built, and the
        // coin range grows all walk, so asking per action kind rescanned it several times over.
        val presentKeys = coinKeysOnBestChain()
        val present = coinsOnBestChain(presentKeys)

        // Only a coin the chain holds and no live entry has claimed can be spent.
        val unclaimed = present.filterNot { coinKeyOf(it) in claimed }

        unclaimed.forEach { add(FuzzAction.RegisterSpend(it)) }

        // A coin in and nothing trackable out — `direct_offboard_coin_into_external_asset`. Rules 5 and 6
        // exist for this shape: with no output to look for, the only evidence the entry ran is its own input
        // being gone, and an entry that has an output is short-circuited away from them by Rule 3b.
        unclaimed.forEach { add(FuzzAction.RegisterOffboard(it)) }

        // `split`: one coin in, several out. The rules quantify over outputs, so an entry with one of them
        // cannot tell `any` from `all`.
        unclaimed.forEach { add(FuzzAction.RegisterSplit(it)) }

        // A voucher can be unloaded only where the pallet would allow it: still a member of its recycler,
        // placed in a ring, and not already unloaded there.
        val unloadable = VOUCHER_POOL.filter { unloadableOnBestChain(it) && voucherKeyOf(it) !in claimed }

        unloadable.forEach { add(FuzzAction.RegisterUnload(it)) }

        // `unload_recycler_into_coins` takes several vouchers of one ring at once. Two inputs is what makes
        // Rule 6's `∃ i. exists(i, F) ∧ ∀ i. absent(i, B)` differ from Rule 5's `∀ i. absent(i, F)`.
        if (unloadable.size >= 2) add(FuzzAction.RegisterMultiUnload(unloadable[0], unloadable[1]))

        // Archival removes the membership, which is the only thing that takes a voucher out of a recycler.
        VOUCHER_POOL.filter { isMemberOnBestChain(it) }.forEach { add(FuzzAction.ArchiveRecycler(it)) }

        // The ring gets built and the queued voucher takes a place in it.
        VOUCHER_POOL.filter { isOnboardingOnBestChain(it) }
            .forEach { voucher -> RINGS.forEach { add(FuzzAction.PlaceVoucherInRing(voucher, it)) } }

        // A coin whose key has left for a peer, and the peer eventually spending it. Only the holder of a
        // key can spend it, which is what makes the handoff mark the app's only record that it is gone.
        val handedOff = harness.repository.getHandoffKeys().getOrThrow()
        val spendable = present.filterNot { coinKeyOf(it) in claimed || coinKeyOf(it) in handedOff }

        spendable.forEach { add(FuzzAction.HandOff(it)) }
        present.filter { coinKeyOf(it) in handedOff }.forEach { add(FuzzAction.PeerSpends(it)) }

        // Two transactions recorded as one operation: either both land or neither does.
        if (spendable.size >= 2) add(FuzzAction.RegisterBatch(spendable[0], spendable[1]))

        add(FuzzAction.Crash)
        add(FuzzAction.Relaunch)

        // Loading a recycler is the only way a voucher comes into being, and without it the pool only ever
        // shrinks: unloads and archival remove vouchers, so voucher actions starve part-way through a walk.
        // Only an index the chain has never held: the seeds already exist, and minting a key that is
        // already on chain is a state the app cannot produce, since it derives a fresh one every time.
        val free = MINTABLE_VOUCHERS.firstOrNull { voucherKeyOf(it) !in mintedKeys }
        if (free != null) {
            present.filterNot { coinKeyOf(it) in claimed }
                .forEach { add(FuzzAction.RegisterVoucherMint(it, free)) }

            // The same voucher arriving from outside the system, with no input to consume.
            add(FuzzAction.RegisterExternalLoad(free))
        }

        // A block may carry an extrinsic only where the runtime would accept it: inside its era, not already
        // applied, and with its input still there to be taken as the origin. A reorg puts a consumed input
        // back, which is what makes an already-included transaction includable again.
        entries.filter { entry ->
            entry.status.isLive &&
                !isIncludedAnywhere(entry) &&
                withinEra(entry) &&
                inputsSpendableOnBestChain(entry, presentKeys) &&
                entry.outputs.none { it.publicKey in presentKeys }
        }.forEach { entry ->
            add(FuzzAction.IncludeTx(entry.id.value, ExtrinsicOutcome.SUCCESS))
            add(FuzzAction.IncludeTx(entry.id.value, ExtrinsicOutcome.FAILURE))
        }
    }

    private suspend fun apply(action: FuzzAction) = when (action) {
        FuzzAction.ProduceBlock -> harness.chain.produceBlock()
        FuzzAction.FinalizeToBest -> harness.finalizeToBest()
        FuzzAction.RunPass -> harness.runPass()

        is FuzzAction.RunPassWithFault -> {
            val healthy = harness.chain.faults
            harness.chain.faults = healthy.with(action.fault)
            // Not `runPass()`: a pass that cannot pin returns a failure rather than a verdict, and that is
            // the outcome this action is here to produce.
            harness.recoveryPass.run()
            harness.chain.faults = healthy
            Unit
        }
        is FuzzAction.Reorg -> harness.reorgLastBlocks(action.depth)

        is FuzzAction.RegisterSpend -> {
            harness.register(inputCoin = action.coin, outputCoin = nextCoin++, periodBlocks = FUZZ_MORTAL_PERIOD)
            harness.releaseSubmissions()
        }

        is FuzzAction.RegisterVoucherMint -> {
            harness.registerVoucherMint(action.coin, action.voucher, periodBlocks = FUZZ_MORTAL_PERIOD)
            mintedKeys += voucherKeyOf(action.voucher)
            harness.releaseSubmissions()
        }

        is FuzzAction.RegisterExternalLoad -> {
            harness.registerExternalVoucherLoad(action.voucher, periodBlocks = FUZZ_MORTAL_PERIOD)
            mintedKeys += voucherKeyOf(action.voucher)
            harness.releaseSubmissions()
        }

        is FuzzAction.RegisterUnload -> {
            harness.registerVoucherUnload(action.voucher, outputCoin = nextCoin++, periodBlocks = FUZZ_MORTAL_PERIOD)
            harness.releaseSubmissions()
        }

        is FuzzAction.RegisterOffboard -> {
            harness.registerOffboard(action.coin, periodBlocks = FUZZ_MORTAL_PERIOD)
            harness.releaseSubmissions()
        }

        is FuzzAction.RegisterSplit -> {
            harness.registerSplit(action.coin, nextCoin++, nextCoin++, periodBlocks = FUZZ_MORTAL_PERIOD)
            harness.releaseSubmissions()
        }

        is FuzzAction.RegisterMultiUnload -> {
            harness.registerVoucherUnload(
                vouchers = listOf(action.first, action.second),
                outputCoin = nextCoin++,
                periodBlocks = FUZZ_MORTAL_PERIOD,
            )
            harness.releaseSubmissions()
        }

        is FuzzAction.HandOff -> {
            harness.service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(action.coin))))
                .getOrNull()
                ?.commit()
                ?.onSuccess { committedMarks += coinKeyOf(action.coin) }
            Unit
        }

        is FuzzAction.PeerSpends -> {
            harness.chain.produceBlock { it.consumeCoin(coinKeyOf(action.coin)) }
            Unit
        }

        is FuzzAction.RegisterBatch -> {
            // Its own window, like every other registration here, and its own group id: a shared one would
            // make separate operations look like halves of the same one, which is what groupId exists to
            // distinguish. Derived from the coins rather than a counter, so it is a function of the action
            // alone and a shrunk trace that drops an earlier batch still replays to the same ids.
            harness.registerGroup(
                action.first to nextCoin++,
                action.second to nextCoin++,
                groupId = CoinageOperationGroupId("fuzz-group-${action.first}-${action.second}"),
                periodBlocks = FUZZ_MORTAL_PERIOD,
            )
            harness.releaseSubmissions()
        }

        FuzzAction.Crash -> harness.crash()

        FuzzAction.Relaunch -> harness.relaunch()

        is FuzzAction.PlaceVoucherInRing -> {
            val member = voucherKeyOf(action.voucher)
            val denomination = bestState()?.recyclerMembers?.getValue(member) ?: error("voucher is in no recycler")

            harness.chain.produceBlock { it.joinRecycler(member, denomination, includedPosition(action.ring)) }
            Unit
        }

        is FuzzAction.ArchiveRecycler -> {
            harness.archiveRecyclerOf(action.voucher, finality = TestActionFinality.IN_BEST)
            Unit
        }

        is FuzzAction.IncludeTx -> {
            val entry = entryOf(action.entryId)

            // Resolved before the block is built: deriving an alias key reads the chain, which the mutation
            // itself cannot do.
            val spentAliases = entry.inputs.filter { it.isVoucher }.map {
                harness.currentAliasKeyOf(it.voucherIndex()) ?: error("no alias key for a voucher being unloaded")
            }

            harness.chain.produceBlock(body = listOf(entry.txHash)) { state ->
                val applied = state.applied(entry.txHash, action.outcome)

                // A failed dispatch is applied but changes nothing this subsystem reads: `post_dispatch`
                // reinserts the coin into `CoinsByOwner` byte for byte, and the aliases an unload would have
                // written are rolled back with the dispatch. The `LockedCoins` row it leaves behind is a
                // separate map the spec puts out of scope.
                if (action.outcome != ExtrinsicOutcome.SUCCESS) {
                    applied
                } else {
                    entry.outputs.fold(applied, ::mintOutput)
                        // A spent voucher stays at its member key; the alias reading unloaded is what says it
                        // is gone. A spent coin simply disappears.
                        .let { next -> spentAliases.fold(next) { acc, key -> acc.withAlias(key, OnChainAliasState.Unloaded) } }
                        .let { next -> entry.inputs.filter { it.isCoin }.fold(next) { acc, i -> acc.consumeCoin(i.publicKey) } }
                }
            }
            Unit
        }
    }

    /**
     * A freshly loaded voucher is a member of its denomination's recycler but is not in a ring yet: rings are
     * built from the queue, so it arrives in `Onboarding` and reaches a ring later. Until then it cannot be
     * unloaded, and its alias is answered without a read — an unload writes under a ring index, so a voucher
     * that has never had one cannot have been unloaded.
     */
    private fun mintOutput(state: CoinageChainState, output: LedgerAsset): CoinageChainState =
        if (output.isVoucher) {
            state.joinRecycler(
                member = output.publicKey,
                denomination = ValueExponent(denominationOf(output.voucherIndex())),
                position = onboardingPosition(),
            )
        } else {
            state.mintCoin(output.publicKey, value = 1, age = 0)
        }

    /**
     * A function of the index alone, so a shrunk trace replays to the same denominations.
     *
     * More than one of them because the alias key is built from the denomination and the ring index: with a
     * single value of each, a rule reading the wrong one would still read the right key.
     */
    private fun denominationOf(voucher: Int) = DENOMINATIONS[voucher % DENOMINATIONS.size]

    private suspend fun checkInvariants(trace: List<FuzzAction>) {
        val entries = harness.repository.getAllEntries().getOrThrow()
        val canonical = canonicalTransactions()
        val finalized = harness.chain.chain.finalizedHead.number
        val cleanPass = trace.lastOrNull() == FuzzAction.RunPass

        entries.forEach { entry ->
            val inclusion = canonical[entry.txHash]

            terminalVerdictIsFrozen(entry, trace)
            failureIsJustified(entry, inclusion, finalized, trace)
            successRestsOnACanonicalDispatch(entry, inclusion, finalized, trace)
            failedEntryMintedNothing(entry, trace)
            undecidedVerdictCarriesNoRecord(entry, trace)
            if (cleanPass) {
                recordReferencesCanonicalBlockForLiveEntries(entry, trace)
                expiredEntriesAreDecided(entry, trace)
                finalizedSuccessIsNotWithheld(entry, inclusion, finalized, trace)
            }
        }

        handoffMarksAreHonoured(entries, trace)
        everyAssetHasOneLiveClaimant(entries, trace)
    }

    /**
     * The spec freezes the verdict, and the verdict is the pair: a terminal entry whose record changed
     * underneath would leave its outputs' selectability resting on something new.
     */
    private fun terminalVerdictIsFrozen(entry: LedgerEntry, trace: List<FuzzAction>) {
        val verdict = entry.status to entry.successDetectedAt

        terminalSeen[entry.id.value]?.let { previous ->
            assert(verdict == previous, trace) {
                "entry ${entry.id.value} changed from terminal $previous to $verdict"
            }
        }
        if (!entry.status.isLive) terminalSeen[entry.id.value] = verdict
    }

    /**
     * FAILURE is terminal and hands the user back an input the chain may already have consumed, so it needs
     * finalized evidence: a dispatch that failed at or below the finalized head, or a window closed there
     * for a transaction that had not already succeeded under it.
     *
     * Anything seen only at the best head can still be reorged away — a success not yet finalized, a failed
     * dispatch in a block that loses, a transaction not included yet — so none of those justify it.
     */
    private fun failureIsJustified(
        entry: LedgerEntry,
        inclusion: CanonicalInclusion?,
        finalized: Long,
        trace: List<FuzzAction>,
    ) {
        if (entry.status != FAILURE) return

        val canNoLongerRunAndNeverDid = windowClosed(entry) && inclusion?.succeededBelow(finalized) != true

        assert(inclusion?.failedBelow(finalized) == true || canNoLongerRunAndNeverDid, trace) {
            "entry ${entry.id.value} was failed with nothing final to justify it: no dispatch failed below " +
                "the finalized head, and it either still has time to execute or has already succeeded"
        }
    }

    /**
     * A finalized success means the transaction really is in a canonical block at or below the finalized
     * head and dispatched successfully there — nothing weaker. Accepting any inclusion would let a success
     * stand on a dispatch that failed, and accepting one above the finalized head would let it stand on a
     * block a reorg can still take away.
     */
    private fun successRestsOnACanonicalDispatch(
        entry: LedgerEntry,
        inclusion: CanonicalInclusion?,
        finalized: Long,
        trace: List<FuzzAction>,
    ) {
        if (entry.status != FINALIZED_SUCCESS) return

        assert(inclusion?.succeededBelow(finalized) == true, trace) {
            "entry ${entry.id.value} is finalized but no canonical block at or below the finalized head " +
                "carries it with a successful dispatch"
        }
    }

    /**
     * A failed entry never ran, so nothing it would have minted can be on the chain.
     *
     * If one is, the ledger has tombstoned a coin the chain still holds: it counts nowhere, and the user
     * cannot reach it again without the rescan this subsystem deliberately does not do.
     */
    private fun failedEntryMintedNothing(entry: LedgerEntry, trace: List<FuzzAction>) {
        if (entry.status != FAILURE) return

        val present = entry.outputs.filter { it.publicKey in coinKeysAtFinalizedHead() }

        assert(present.isEmpty(), trace) {
            "entry ${entry.id.value} was failed but the finalized chain still holds ${present.size} of its outputs"
        }
    }

    /** The record is what keeps outputs optimistically spendable, so a verdict claiming nothing carries nothing. */
    private fun undecidedVerdictCarriesNoRecord(entry: LedgerEntry, trace: List<FuzzAction>) {
        if (entry.status != PENDING && entry.status != FAILURE) return

        assert(entry.successDetectedAt == null, trace) {
            "entry ${entry.id.value} is ${entry.status} but still carries a detected-success record"
        }
    }

    private fun recordReferencesCanonicalBlockForLiveEntries(entry: LedgerEntry, trace: List<FuzzAction>) {
        // Terminal entries may store dangling blocks: successDetectedAt block can be far beyond actual block tx landed, which could even be finalized
        // Meaning successDetectedAt could be reorged with tx still being finalized
        if (!entry.status.isLive) return

        val recorded = entry.successDetectedAt ?: return
        val canonicalHash = harness.chain.chain.canonicalAt(recorded.blockNumber)?.hash

        assert(canonicalHash == recorded.blockHash, trace) {
            "entry ${entry.id.value} kept a record of block ${recorded.blockNumber} across a pass, but that " +
                "block is not the canonical one"
        }
    }

    /**
     * Liveness, which nothing else here checks: every other invariant is a safety property, and a ladder
     * that answered PENDING forever would satisfy all of them.
     *
     * Past its window an entry must be decided, and a pass with no failing read has what it needs to decide
     * it: Rules 3b and 4b cannot fire outside the window, so the entry reaches the body search, whose range
     * covers the whole era. Found is a verdict; not found across a fully read range past mortality is
     * FAILURE. Neither leaves PENDING.
     *
     * Two consecutive passes, because one is not a fixpoint. A pass evaluates every entry against one
     * snapshot, so a verdict that unblocks another entry only reaches it on the next pass — the same reason
     * a chain of promotions advances one hop per pass. The case that made this concrete: an entry demoted
     * from PENDING_SUCCESS by Rule 0, because a reorg took the block it had been recorded in, spends that
     * pass being demoted and is failed by the one after.
     *
     * Inside the window PENDING is correct even for a transaction the chain has already answered: a failed
     * dispatch leaves the input in place, so Rule 4b short-circuits on "it can still execute" before the
     * search runs. That is the documented trade-off, and it costs the entry its window.
     */
    private fun expiredEntriesAreDecided(entry: LedgerEntry, trace: List<FuzzAction>) {
        if (!windowClosed(entry)) return

        val passesSinceExpiry = expiredPasses.merge(entry.id.value, 1, Int::plus)!!

        assert(entry.status != PENDING || passesSinceExpiry < PASSES_TO_DECIDE, trace) {
            "entry ${entry.id.value} is still PENDING after $passesSinceExpiry clean passes past its window, " +
                "which closed at ${entry.mortalityEnd} with the finalized head at " +
                "${harness.chain.chain.finalizedHead.number}"
        }
    }

    /**
     * PENDING_SUCCESS is a verdict about a best head: it says the transaction was seen somewhere that can
     * still be rewritten. Once the finalized head reaches the block the transaction actually succeeded in,
     * nothing about it can be rewritten any more, so the entry must stop being pending.
     *
     * Counted in passes for the same reason as [expiredEntriesAreDecided], and worth checking separately
     * from it: this state can be reached far inside the mortality window, so the expiry check never sees it.
     */
    private fun finalizedSuccessIsNotWithheld(
        entry: LedgerEntry,
        inclusion: CanonicalInclusion?,
        finalized: Long,
        trace: List<FuzzAction>,
    ) {
        if (inclusion?.succeededBelow(finalized) != true) return

        val passesSinceSettled = settledPasses.merge(entry.id.value, 1, Int::plus)!!

        assert(entry.status == FINALIZED_SUCCESS || passesSinceSettled < PASSES_TO_DECIDE, trace) {
            "entry ${entry.id.value} is ${entry.status} after $passesSinceSettled clean passes, though " +
                "its transaction succeeded in block ${inclusion.blockNumber}, below the finalized head at $finalized"
        }
    }

    /**
     * A key that has left for a peer can be spent by them at any moment, so nothing of ours may claim it,
     * and a relaunch must not release a mark whose keys really did leave.
     */
    private suspend fun handoffMarksAreHonoured(entries: List<LedgerEntry>, trace: List<FuzzAction>) {
        val marks = harness.repository.getHandoffKeys().getOrThrow()

        entries.filter { it.status != FAILURE }.forEach { entry ->
            assert(entry.inputs.none { it.publicKey in marks }, trace) {
                "entry ${entry.id.value} claims an asset that was handed off to a peer"
            }
        }
        assert(marks.containsAll(committedMarks), trace) {
            "a committed handoff mark did not survive: ${committedMarks - marks}"
        }
    }

    private fun everyAssetHasOneLiveClaimant(entries: List<LedgerEntry>, trace: List<FuzzAction>) {
        val claims = entries.filter { it.status != FAILURE }.flatMap { entry -> entry.inputs.map { it.publicKey } }

        assert(claims.size == claims.distinct().size, trace) {
            "an asset is claimed by more than one entry that is not a failure"
        }
    }

    private fun assert(condition: Boolean, trace: List<FuzzAction>, message: () -> String) {
        if (!condition) throw FuzzViolation(message(), trace)
    }

    /**
     * One read failing for the length of one pass.
     *
     * A batched read has no per-key error channel, so the faithful shape is the whole read failing rather
     * than one key of it. Coin reads are silenced by their block: a pass pins `F` and `B` once, so those two
     * hashes are every block its coin reads ask for.
     */
    private suspend fun ChainReadFaults.with(fault: FuzzFault): ChainReadFaults = when (fault) {
        FuzzFault.COINS -> copy(
            statelessBlocks = statelessBlocks + setOf(harness.chain.chain.finalizedHead.hash, harness.chain.chain.bestHead.hash),
        )

        FuzzFault.ALIASES -> copy(unreadableAliases = unreadableAliases + VOUCHER_POOL.mapNotNull { harness.currentAliasKeyOf(it) })

        FuzzFault.MEMBERSHIPS -> copy(membershipsUnreadable = true)

        FuzzFault.RING_POSITIONS -> copy(ringPositionsUnreadable = true)

        // Takes out the body search and the hash lookup Rule 0 checks its record against.
        FuzzFault.BLOCKS -> copy(everyBlockUnreadable = true)

        FuzzFault.OUTCOMES -> copy(
            unreadableOutcomes = unreadableOutcomes + harness.repository.getAllEntries().getOrThrow().map { it.txHash },
        )

        FuzzFault.PIN -> copy(pinFails = true)
    }

    // ---- ground truth, read from the chain rather than from a second model ---------------------------

    private fun coinsOnBestChain(keys: Set<AccountId> = coinKeysOnBestChain()): List<Int> =
        (SEED_COINS + (FIRST_OUTPUT_COIN until nextCoin)).filter { coinKeyOf(it) in keys }

    /**
     * Spendable at the best head. A coin must be in `CoinsByOwner`; a voucher must still be a member of a
     * recycler, in a ring, with no alias saying it was already unloaded.
     */
    private suspend fun inputsSpendableOnBestChain(
        entry: LedgerEntry,
        coins: Set<AccountId> = coinKeysOnBestChain(),
    ): Boolean =
        entry.inputs.all { input ->
            if (input.isCoin) input.publicKey in coins else unloadableOnBestChain(input.voucherIndex())
        }

    /** Only ever called for an asset the ledger recorded as a voucher of ours, which always carries one. */
    private fun LedgerAsset.voucherIndex(): Int = (asset as OwnAsset.Voucher).ringVrfIndex.item

    private fun isOnboardingOnBestChain(voucher: Int) =
        bestState()?.ringPositions?.get(voucherKeyOf(voucher)) is RingPosition.Onboarding

    private fun isMemberOnBestChain(voucher: Int) =
        bestState()?.recyclerMembers?.containsKey(voucherKeyOf(voucher)) == true

    private suspend fun unloadableOnBestChain(voucher: Int): Boolean {
        if (!isMemberOnBestChain(voucher)) return false
        val aliasKey = harness.currentAliasKeyOf(voucher) ?: return false

        return bestState()?.aliases?.get(aliasKey) == null
    }

    private fun bestState() = harness.chain.chain.stateAt(harness.chain.chain.bestHead.hash)

    private fun coinKeysAtFinalizedHead() =
        harness.chain.chain.stateAt(harness.chain.chain.finalizedHead.hash)?.coins?.keys.orEmpty()

    private fun coinKeysOnBestChain() =
        harness.chain.chain.stateAt(harness.chain.chain.bestHead.hash)?.coins?.keys.orEmpty()

    /**
     * Every transaction the canonical chain carries, with where it landed and how it dispatched.
     *
     * Maintained as the walk runs rather than rebuilt on each look. Rebuilding walked every block for every
     * action, which is quadratic in a walk's length for an answer that only changes when a block arrives.
     *
     * Nothing about the finalized head is stored: whether an inclusion is below it is a comparison at query
     * time, so finalizing — a large share of all actions — never has to touch this.
     */
    private fun canonicalTransactions(): Map<String, CanonicalInclusion> {
        val chain = harness.chain.chain

        // A reorg replaces blocks that were already folded in, so the index is rebuilt rather than repaired:
        // an index that disagrees with the chain would have the oracle judging against a fiction. Detected by
        // the hash and not the height, because a reorg followed by an equally long branch leaves the height
        // where it was.
        if (indexedUpTo >= 0 && chain.canonicalAt(indexedUpTo)?.hash != indexedHash) {
            canonicalIndex.clear()
            indexedUpTo = -1
            indexedHash = null
        }

        while (indexedUpTo < chain.bestHead.number) {
            val block = chain.canonicalAt(indexedUpTo + 1) ?: break

            // First inclusion wins, which ascending order gives for free.
            block.body.forEach { txHash ->
                canonicalIndex.putIfAbsent(txHash, CanonicalInclusion(block.number, block.state.outcomes[txHash]))
            }

            indexedUpTo = block.number
            indexedHash = block.hash
        }

        return canonicalIndex
    }

    /**
     * The transaction can no longer execute, judged at the finalized head.
     *
     * Stated in the chain's terms rather than by reusing the rules' own predicate, so that this stays an
     * independent check rather than a restatement of the code under test.
     */
    private fun windowClosed(entry: LedgerEntry): Boolean =
        harness.chain.chain.finalizedHead.number > entry.mortalityEnd

    private fun isIncludedAnywhere(entry: LedgerEntry) = entry.txHash in canonicalTransactions()

    /**
     * The block that would carry the extrinsic is inside its era.
     *
     * Judged on the block about to be produced, not the current head: producing one appends at best + 1, so
     * checking the head lets a transaction land a block after its era expired — which the runtime would
     * reject, and which makes the search's range end before the block it landed in.
     */
    private fun withinEra(entry: LedgerEntry) =
        harness.chain.chain.bestHead.number + 1 in entry.checkpoint.blockNumber..entry.mortalityEnd

    private suspend fun entryOf(id: Long) = harness.repository.getAllEntries().getOrThrow().first { it.id.value == id }

    private companion object {
        val SEED_COINS = listOf(1, 2, 3)
        val SEED_VOUCHERS = listOf(7, 8)

        /**
         * Wide enough that a walk never runs out. An index is offered once and never returned, so a pool of
         * eight was exhausted part-way through every walk and voucher actions stopped being drawn at all.
         */
        val MINTABLE_VOUCHERS = (9..128).toList()
        val VOUCHER_POOL = SEED_VOUCHERS + MINTABLE_VOUCHERS
        const val SEED_DENOMINATION = 3
        const val SEED_RING = 5
        val DENOMINATIONS = listOf(3, 4)
        val RINGS = listOf(5, 6)
        const val FIRST_OUTPUT_COIN = 100

        /**
         * Short on purpose. At the production window of 128 blocks a walk never produces enough blocks for
         * mortality to expire, so every rule that turns on a closed window went unexercised.
         */
        const val FUZZ_MORTAL_PERIOD = 12

        /**
         * One pass is not a fixpoint: an entry demoted by Rule 0 spends that pass being demoted and is
         * decided by the next. Two is the whole cascade for an expired entry, because past its window the
         * body search decides it on its own evidence rather than on another entry's verdict.
         */
        const val PASSES_TO_DECIDE = 2
    }
}

/** Where a transaction landed on the canonical chain, and what its dispatch did there. */
private data class CanonicalInclusion(val blockNumber: Long, val outcome: ExtrinsicOutcome?) {
    fun succeededBelow(finalizedHead: Long) = blockNumber <= finalizedHead && outcome == ExtrinsicOutcome.SUCCESS

    fun failedBelow(finalizedHead: Long) = blockNumber <= finalizedHead && outcome == ExtrinsicOutcome.FAILURE
}

/**
 * A read that fails for the length of one pass.
 *
 * Every one of these leaves the ledger a state it cannot judge, which is the whole point: the rules are
 * written so that an unknown withholds a verdict rather than producing one. A walk with no failing read
 * never reaches any of those branches.
 */
enum class FuzzFault { COINS, ALIASES, MEMBERSHIPS, RING_POSITIONS, BLOCKS, OUTCOMES, PIN }

/** What a walk can do. */
sealed interface FuzzAction {
    val kind: FuzzActionKind

    data object ProduceBlock : FuzzAction {
        override val kind = FuzzActionKind.PRODUCE_BLOCK
    }

    data object FinalizeToBest : FuzzAction {
        override val kind = FuzzActionKind.FINALIZE
    }

    data object RunPass : FuzzAction {
        override val kind = FuzzActionKind.RUN_PASS
    }

    /** A pass with one read failing throughout it, so every evidence path has an unknown to handle. */
    data class RunPassWithFault(val fault: FuzzFault) : FuzzAction {
        override val kind = FuzzActionKind.FAULTY_PASS
    }

    data class Reorg(val depth: Int) : FuzzAction {
        override val kind = FuzzActionKind.REORG
    }

    data class RegisterSpend(val coin: Int) : FuzzAction {
        override val kind = FuzzActionKind.REGISTER_SPEND
    }

    data class RegisterUnload(val voucher: Int) : FuzzAction {
        override val kind = FuzzActionKind.REGISTER_UNLOAD
    }

    /** Several vouchers of one ring in, one coin out. */
    data class RegisterMultiUnload(val first: Int, val second: Int) : FuzzAction {
        override val kind = FuzzActionKind.REGISTER_MULTI_UNLOAD
    }

    /** One coin in, nothing trackable out. */
    data class RegisterOffboard(val coin: Int) : FuzzAction {
        override val kind = FuzzActionKind.REGISTER_OFFBOARD
    }

    /** One coin in, two coins out. */
    data class RegisterSplit(val coin: Int) : FuzzAction {
        override val kind = FuzzActionKind.REGISTER_SPLIT
    }

    data class RegisterVoucherMint(val coin: Int, val voucher: Int) : FuzzAction {
        override val kind = FuzzActionKind.MINT_VOUCHER
    }

    data class RegisterExternalLoad(val voucher: Int) : FuzzAction {
        override val kind = FuzzActionKind.EXTERNAL_LOAD
    }

    data class HandOff(val coin: Int) : FuzzAction {
        override val kind = FuzzActionKind.HAND_OFF
    }

    data class PeerSpends(val coin: Int) : FuzzAction {
        override val kind = FuzzActionKind.PEER_SPENDS
    }

    data class RegisterBatch(val first: Int, val second: Int) : FuzzAction {
        override val kind = FuzzActionKind.REGISTER_BATCH
    }

    data object Crash : FuzzAction {
        override val kind = FuzzActionKind.CRASH
    }

    data object Relaunch : FuzzAction {
        override val kind = FuzzActionKind.RELAUNCH
    }

    data class ArchiveRecycler(val voucher: Int) : FuzzAction {
        override val kind = FuzzActionKind.ARCHIVE
    }

    /** The queued voucher takes a place in a ring, which is what makes it unloadable. */
    data class PlaceVoucherInRing(val voucher: Int, val ring: Int) : FuzzAction {
        override val kind = FuzzActionKind.PLACE_IN_RING
    }

    /** Identified by the entry's id rather than the entry, so a recorded walk survives a replay. */
    data class IncludeTx(val entryId: Long, val outcome: ExtrinsicOutcome) : FuzzAction {
        override val kind = FuzzActionKind.INCLUDE_TX
    }
}

enum class FuzzActionKind {
    PRODUCE_BLOCK, FINALIZE, REORG, RUN_PASS, FAULTY_PASS, REGISTER_SPEND, REGISTER_UNLOAD, REGISTER_MULTI_UNLOAD,
    REGISTER_OFFBOARD, REGISTER_SPLIT, MINT_VOUCHER, EXTERNAL_LOAD,
    ARCHIVE, PLACE_IN_RING, INCLUDE_TX, HAND_OFF, PEER_SPENDS, REGISTER_BATCH, CRASH, RELAUNCH
}

/**
 * How often a walk takes each direction, so one profile can hammer spending and another chain instability.
 *
 * Weighting is per kind rather than per enabled action: at any moment there may be a dozen coins to spend
 * and one legal reorg depth, and weighting the flat list would let whichever kind happens to have the most
 * instances dominate however it was configured. A kind is chosen first, then one of its actions uniformly.
 * A weight of zero removes that direction entirely.
 */
data class FuzzProfile(val name: String, private val weights: Map<FuzzActionKind, Int>) {
    fun pick(random: Random, enabled: List<FuzzAction>): FuzzAction? {
        val byKind = enabled.groupBy { it.kind }.filterKeys { weightOf(it) > 0 }
        if (byKind.isEmpty()) return null

        val total = byKind.keys.sumOf { weightOf(it) }
        var roll = random.nextInt(total)
        val kind = byKind.keys.first { roll -= weightOf(it); roll < 0 }
        val choices = byKind.getValue(kind)

        return choices[random.nextInt(choices.size)]
    }

    private fun weightOf(kind: FuzzActionKind) = weights[kind] ?: 0

    companion object {
        private const val ORDINARY = 10

        /**
         * A faulty pass is an extra pass that mostly withholds a verdict, so at [ORDINARY] it roughly halves
         * how much a walk decides. Rare in the everyday mix; [FLAKY_READS] is where it is the subject.
         */
        private const val RARE_FAULTS = 3

        /**
         * Weights are overrides on a base where every direction is [ORDINARY].
         *
         * A profile naming only what it emphasises means a newly added action kind is exercised everywhere
         * by default, rather than silently sitting at zero in every profile that predates it.
         */
        private fun profile(name: String, vararg overrides: Pair<FuzzActionKind, Int>) =
            FuzzProfile(
                name,
                FuzzActionKind.entries.associateWith { ORDINARY } +
                    (FuzzActionKind.FAULTY_PASS to RARE_FAULTS) +
                    overrides,
            )

        /**
         * The everyday mix. Restarts and archival are deliberately rare here: both reset or destroy a lot of
         * state, and at ordinary weight they dominate a walk and stop it building anything deep. They get
         * profiles of their own instead.
         */
        val BALANCED = profile(
            "balanced",
            FuzzActionKind.CRASH to 1,
            FuzzActionKind.RELAUNCH to 1,
            FuzzActionKind.ARCHIVE to 1,
        )

        /** Many transactions in flight at once, competing for the same assets. */
        val HEAVY_SPENDING = profile(
            "heavy spending",
            FuzzActionKind.REGISTER_SPEND to 30, FuzzActionKind.INCLUDE_TX to 30,
            FuzzActionKind.REGISTER_BATCH to 20, FuzzActionKind.RUN_PASS to 15,
            FuzzActionKind.REORG to 2, FuzzActionKind.ARCHIVE to 1,
            FuzzActionKind.CRASH to 1, FuzzActionKind.RELAUNCH to 1,
        )

        /** A deep unfinalized suffix, rewritten under entries that were decided on it. */
        val REORG_STORM = profile(
            "reorg storm",
            FuzzActionKind.REORG to 30, FuzzActionKind.PRODUCE_BLOCK to 30,
            FuzzActionKind.FINALIZE to 3, FuzzActionKind.RUN_PASS to 20,
            FuzzActionKind.INCLUDE_TX to 15, FuzzActionKind.ARCHIVE to 1,
            FuzzActionKind.CRASH to 1, FuzzActionKind.RELAUNCH to 1,
        )

        /** Vouchers loaded, unloaded and minted, with rings left alone so the population survives. */
        val VOUCHER_CHURN = profile(
            "voucher churn",
            FuzzActionKind.REGISTER_UNLOAD to 30, FuzzActionKind.MINT_VOUCHER to 25,
            FuzzActionKind.EXTERNAL_LOAD to 25, FuzzActionKind.INCLUDE_TX to 20,
            FuzzActionKind.REGISTER_SPEND to 3, FuzzActionKind.ARCHIVE to 5,
            FuzzActionKind.CRASH to 1, FuzzActionKind.RELAUNCH to 1,
        )

        /** Rings archived under unloads in flight, which is the voucher-specific hazard. */
        val ARCHIVAL_CHURN = profile(
            "archival churn",
            FuzzActionKind.ARCHIVE to 30, FuzzActionKind.REGISTER_UNLOAD to 20,
            FuzzActionKind.EXTERNAL_LOAD to 20, FuzzActionKind.INCLUDE_TX to 15,
            FuzzActionKind.REORG to 15, FuzzActionKind.REGISTER_SPEND to 3,
            FuzzActionKind.CRASH to 1, FuzzActionKind.RELAUNCH to 1,
        )

        /** The process dying constantly, so nothing in memory is ever trusted for long. */
        val RESTART_CHURN = profile(
            "restart churn",
            FuzzActionKind.CRASH to 25, FuzzActionKind.RELAUNCH to 25,
            FuzzActionKind.RUN_PASS to 20, FuzzActionKind.REGISTER_SPEND to 15,
            FuzzActionKind.INCLUDE_TX to 15, FuzzActionKind.HAND_OFF to 10,
            FuzzActionKind.ARCHIVE to 1,
        )

        /** Keys leaving for peers and being spent there, which the app only knows about from its own mark. */
        val HANDOFF_CHURN = profile(
            "handoff churn",
            FuzzActionKind.HAND_OFF to 30, FuzzActionKind.PEER_SPENDS to 25,
            FuzzActionKind.REGISTER_SPEND to 20, FuzzActionKind.INCLUDE_TX to 20,
            FuzzActionKind.RUN_PASS to 15, FuzzActionKind.ARCHIVE to 1,
            FuzzActionKind.CRASH to 2, FuzzActionKind.RELAUNCH to 2,
        )

        /** Reads failing constantly, so no rule ever gets the whole picture and none may guess at the rest. */
        val FLAKY_READS = profile(
            "flaky reads",
            FuzzActionKind.FAULTY_PASS to 30, FuzzActionKind.RUN_PASS to 15,
            FuzzActionKind.INCLUDE_TX to 15, FuzzActionKind.REGISTER_SPEND to 10,
            FuzzActionKind.ARCHIVE to 1, FuzzActionKind.CRASH to 1, FuzzActionKind.RELAUNCH to 1,
        )

        val ALL = listOf(
            BALANCED, HEAVY_SPENDING, REORG_STORM, VOUCHER_CHURN, ARCHIVAL_CHURN, RESTART_CHURN, HANDOFF_CHURN,
            FLAKY_READS,
        )
    }
}

class FuzzViolation(message: String, val trace: List<FuzzAction>) : AssertionError(message)

/** A candidate the shrinker proposed that is not a legal history, so it says nothing about the violation. */
object ReplayDiverged : Exception("replayed action was not enabled in the state the candidate reached")

/** Prints the shrunk walk as a scenario, so a fuzz failure graduates into a named test. */
fun List<FuzzAction>.asKotlin(): String = joinToString("\n") { action ->
    when (action) {
        FuzzAction.ProduceBlock -> "        advanceBlocks(1, finality = IN_BEST)"
        FuzzAction.FinalizeToBest -> "        finalizeToBest()"
        FuzzAction.RunPass -> "        runPass()"
        is FuzzAction.RunPassWithFault -> "        // ${action.fault} unreadable for one pass\n        runPass()"
        is FuzzAction.Reorg -> "        reorgLastBlocks(${action.depth})"
        is FuzzAction.RegisterSpend -> "        register(inputCoin = ${action.coin}, outputCoin = ?)"
        is FuzzAction.RegisterUnload -> "        registerVoucherUnload(voucher = ${action.voucher}, outputCoin = ?)"
        is FuzzAction.RegisterMultiUnload ->
            "        registerVoucherUnload(vouchers = listOf(${action.first}, ${action.second}), outputCoin = ?)"
        is FuzzAction.RegisterOffboard -> "        registerOffboard(inputCoin = ${action.coin})"
        is FuzzAction.RegisterSplit -> "        registerSplit(inputCoin = ${action.coin}, outputCoins = ?)"
        is FuzzAction.RegisterVoucherMint -> "        registerVoucherMint(${action.coin}, voucher = ${action.voucher})"
        is FuzzAction.RegisterExternalLoad -> "        registerExternalVoucherLoad(voucher = ${action.voucher})"
        is FuzzAction.HandOff -> "        service.preCommitHandoff(listOf(OwnAsset.Coin(${action.coin}))).getOrThrow().commit()"
        is FuzzAction.PeerSpends -> "        consumeCoinOnChain(${action.coin}, finality = IN_BEST)"
        is FuzzAction.RegisterBatch -> "        registerGroup(${action.first} to ?, ${action.second} to ?)"
        FuzzAction.Crash -> "        crash()"
        FuzzAction.Relaunch -> "        relaunch()"
        is FuzzAction.ArchiveRecycler -> "        archiveRecyclerOf(${action.voucher}, finality = IN_BEST)"
        is FuzzAction.PlaceVoucherInRing ->
            "        givenVoucherInRecycler(${action.voucher}, denomination = ?, ring = ${action.ring}, finality = IN_BEST)"
        is FuzzAction.IncludeTx -> "        includeInBlock(txOf(${action.entryId}), ${action.outcome}, finality = IN_BEST)"
    }
}

/** The app starts holding coins and vouchers it did not mint itself, which is what a top-up looks like. */
suspend fun DurabilityHarness.givenFuzzSeedAssets() {
    mintCoinsOnChain(1, 2, 3, finality = TestActionFinality.FINALIZED)
    listOf(7, 8).forEach { givenVoucherInRecycler(it, denomination = 3, ring = 5, finality = TestActionFinality.FINALIZED) }
}
