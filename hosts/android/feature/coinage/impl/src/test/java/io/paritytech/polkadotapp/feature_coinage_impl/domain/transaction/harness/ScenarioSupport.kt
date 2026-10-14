package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.RealCoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.RecyclerAliasKey
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery.ChainEvidence
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery.CoinageEvidenceCollector
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import kotlinx.coroutines.test.runTest
import timber.log.Timber
import java.math.BigInteger

enum class TestActionFinality { FINALIZED, IN_BEST }

/**
 * Runs [body] against a fresh harness over an empty chain at genesis, `F = B = 0`.
 *
 * This is what a JUnit `@Before` would be if it could: the harness needs the `TestScope` that only exists
 * inside `runTest`, so the shared setup has to wrap the test rather than precede it. It produces no block
 * and changes no setting — everything a scenario depends on, the scenario puts there.
 */
fun scenario(body: suspend DurabilityHarness.() -> Unit) = runTest {
    Timber.uprootAll()
    Timber.plant(StdoutTree())
    DurabilityHarness(testScope = this, initialState = CoinageChainState.EMPTY).body()
}

/** Carries the rules' own log lines into the test's captured output, so a run says which rule decided what. */
private class StdoutTree : Timber.DebugTree() {
    override fun log(priority: Int, tag: String?, message: String, t: Throwable?) = println(message)
}

// ---- moving the chain ---------------------------------------------------------------------------------

fun DurabilityHarness.mintCoinsOnChain(vararg coins: Int, finality: TestActionFinality) = produceBlock(finality) { state ->
    coins.fold(state) { acc, coin -> acc.mintCoin(coinKeyOf(coin), value = 1, age = 0) }
}

/** What a peer spending it, or a transaction of ours consuming it, looks like from here. */
fun DurabilityHarness.consumeCoinOnChain(coin: Int, finality: TestActionFinality) =
    produceBlock(finality) { it.consumeCoin(coinKeyOf(coin)) }

/** The unload executed: the alias reads unloaded, which is the only positive proof a voucher was spent. */
suspend fun DurabilityHarness.unloadVoucherOnChain(voucher: Int, finality: TestActionFinality) {
    val key = requireAliasKeyOf(voucher)
    produceBlock(finality) { it.withAlias(key, OnChainAliasState.Unloaded) }
}

/** Writes an alias under a ring the voucher is **not** in, which nothing should ever read. */
suspend fun DurabilityHarness.unloadVoucherAtOtherRing(
    voucher: Int,
    denomination: Int,
    ring: Int,
    finality: TestActionFinality,
) {
    val key = aliasKeyAt(voucher, denomination, ring)
    produceBlock(finality) { it.withAlias(key, OnChainAliasState.Unloaded) }
}

/** Ring cleaning: the alias is gone. A voucher's absence is not consumption — only a coin's is. */
suspend fun DurabilityHarness.cleanVoucherFromRecycler(voucher: Int, finality: TestActionFinality) {
    val key = requireAliasKeyOf(voucher)
    produceBlock(finality) { it.clearAlias(key) }
}

/** Archival: the member-to-denomination entry goes synchronously, before dusting clears any alias. */
suspend fun DurabilityHarness.archiveRecyclerOf(voucher: Int, finality: TestActionFinality) {
    val member = voucherDerivation.memberKeyOf(testKey(voucher))
    produceBlock(finality) { it.leaveRecycler(member) }
}

/**
 * Applies [txHash] in a new block with [outcome], returning the block itself.
 *
 * The block and not its number: a block reorged out keeps its hash but loses its number to whatever replaces
 * it, and a scenario about the wrong block being read needs to hold on to the one it meant.
 */
fun DurabilityHarness.includeInBlock(txHash: String, outcome: ExtrinsicOutcome, finality: TestActionFinality) =
    produceBlock(finality, body = listOf(txHash)) { it.applied(txHash, outcome) }

/** The canonical hash at [blockNumber], for a scenario that must name a block to the watcher. */
fun DurabilityHarness.hashOfBlock(blockNumber: Long): String =
    chain.chain.canonicalAt(blockNumber)?.hash ?: error("no canonical block at $blockNumber")

/** Drops [depth] blocks from the best head. Legal only while they are unfinalized, which is the point. */
fun DurabilityHarness.reorgLastBlocks(depth: Int) = chain.reorg(depth)

// ---- arranging the subsystem --------------------------------------------------------------------------

/**
 * A voucher in a recycler: a member of the denomination's collection, included in a ring, with no alias
 * state — `pallets/coinage`: *"Absence from the map means the alias is available."*
 */
suspend fun DurabilityHarness.givenVoucherInRecycler(
    voucher: Int,
    denomination: Int,
    ring: Int,
    finality: TestActionFinality,
) {
    val member = voucherDerivation.memberKeyOf(testKey(voucher))
    produceBlock(finality) { it.joinRecycler(member, ValueExponent(denomination), includedPosition(ring)) }
}

/** Loaded into the recycler but not yet placed in a ring, so it has no ring index and cannot be unloaded. */
suspend fun DurabilityHarness.givenVoucherOnboarding(voucher: Int, denomination: Int, finality: TestActionFinality) {
    val member = voucherDerivation.memberKeyOf(testKey(voucher))
    produceBlock(finality) { it.joinRecycler(member, ValueExponent(denomination), onboardingPosition()) }
}

/**
 * Suspended from its ring: still a member of the recycler, but holding no ring index.
 *
 * Its alias key cannot be derived without one, so nothing can be said about whether it was unloaded.
 */
suspend fun DurabilityHarness.givenVoucherSuspended(voucher: Int, denomination: Int, finality: TestActionFinality) {
    val member = voucherDerivation.memberKeyOf(testKey(voucher))
    produceBlock(finality) { it.joinRecycler(member, ValueExponent(denomination), RingPosition.Suspended) }
}

/** Registered, and its watcher released — a pass decides only entries submission no longer owns. */
suspend fun DurabilityHarness.givenUnwatchedEntry(inputCoin: Int, outputCoin: Int): CoinageTransactionId {
    val id = register(inputCoin = inputCoin, outputCoin = outputCoin).getOrThrow()
    releaseSubmissions()

    return id
}

/** The same for a voucher unload. */
suspend fun DurabilityHarness.givenUnwatchedVoucherUnload(voucher: Int, outputCoin: Int): CoinageTransactionId {
    val id = registerVoucherUnload(voucher = voucher, outputCoin = outputCoin).getOrThrow()
    releaseSubmissions()

    return id
}

/**
 * The transaction really executed: it sits in a block with a successful dispatch, its input consumed and its
 * output minted. The **ledger has not been told** — the entry stays PENDING until a pass reads the chain.
 */
suspend fun DurabilityHarness.givenEntryExecutedOnChain(
    inputCoin: Int,
    outputCoin: Int,
    finality: TestActionFinality,
): CoinageTransactionId {
    val id = givenUnwatchedEntry(inputCoin = inputCoin, outputCoin = outputCoin)
    val txHash = repository.getEntry(id).getOrThrow()!!.txHash

    produceBlock(finality, body = listOf(txHash)) {
        it.applied(txHash, ExtrinsicOutcome.SUCCESS)
            .consumeCoin(coinKeyOf(inputCoin))
            .mintCoin(coinKeyOf(outputCoin), value = 1, age = 0)
    }

    return id
}

/**
 * The same, plus the pass that reads it, so the ledger has a verdict: FINALIZED_SUCCESS when the block was
 * finalized, PENDING_SUCCESS when it is only in the best chain. The rules derive that from [finality] —
 * this helper does not choose it.
 */
suspend fun DurabilityHarness.givenEntryDecided(
    inputCoin: Int,
    outputCoin: Int,
    finality: TestActionFinality,
): CoinageTransactionId =
    givenEntryExecutedOnChain(inputCoin, outputCoin, finality).also { runPass() }

// ---- faults -------------------------------------------------------------------------------------------

/** Adds to whatever is already unreadable rather than replacing it. */
fun DurabilityHarness.makeCoinsUnreadable(vararg coins: Int) {
    chain.faults = chain.faults.copy(unreadableCoins = chain.faults.unreadableCoins + coins.map { coinKeyOf(it) })
}

fun DurabilityHarness.makeCoinsReadable() {
    chain.faults = chain.faults.copy(unreadableCoins = emptySet())
}

/**
 * Named vouchers rather than "all of them", and it **throws** for one whose alias key cannot be derived
 * from the chain — silencing a key nothing will ask for is never what a caller means.
 */
suspend fun DurabilityHarness.makeVoucherAliasesUnreadable(voucher: Int, vararg more: Int) {
    val keys = (listOf(voucher) + more.toList()).map { requireAliasKeyOf(it) }
    chain.faults = chain.faults.copy(unreadableAliases = chain.faults.unreadableAliases + keys)
}

fun DurabilityHarness.makeAliasesReadable() {
    chain.faults = chain.faults.copy(unreadableAliases = emptySet())
}

/** `RecyclersCoinToRecycler` cannot be read, which is the frequent transient failure on the voucher path. */
fun DurabilityHarness.makeRecyclerMembershipsUnreadable() {
    chain.faults = chain.faults.copy(membershipsUnreadable = true)
}

/** `Members` cannot be read, so a voucher's position in its ring is unknown. */
fun DurabilityHarness.makeRingPositionsUnreadable() {
    chain.faults = chain.faults.copy(ringPositionsUnreadable = true)
}

/**
 * Coin reads fail at the finalized head but still answer at the best one.
 *
 * Reads at the two heads are separate calls, so one can fail while the other succeeds. Anchored to whatever
 * is finalized when this is called, so a scenario moves the chain into place first.
 */
fun DurabilityHarness.makeCoinsUnreadableAtFinalizedHead() {
    chain.faults = chain.faults.copy(statelessBlocks = chain.faults.statelessBlocks + chain.chain.finalizedHead.hash)
}

fun DurabilityHarness.makeBlocksUnreadable(vararg blockNumbers: Long) {
    chain.faults = chain.faults.copy(unreadableBlocks = chain.faults.unreadableBlocks + blockNumbers.toList())
}

/** A standing rule, so blocks produced after this call are unreadable too and call order stops mattering. */
fun DurabilityHarness.makeEveryBlockUnreadable() {
    chain.faults = chain.faults.copy(everyBlockUnreadable = true)
}

/**
 * Takes the body search out of play, so only the rules above it can decide an entry.
 *
 * The search is the last thing the ladder tries and it decides on its own evidence, so any test whose
 * subject is a rule can pass on the search instead and never exercise what it names. Every such test opens
 * with this; the handful that are about the search itself are the only ones that leave it on.
 */
fun DurabilityHarness.disableFallbackTxSearch() {
    chain.faults = chain.faults.copy(txSearchDisabled = true)
}

fun DurabilityHarness.makeBlocksReadable() {
    chain.faults = chain.faults.copy(unreadableBlocks = emptySet(), everyBlockUnreadable = false)
}

/** The extrinsic is found, but the events that say whether its dispatch succeeded are not. */
fun DurabilityHarness.makeOutcomeUnreadable(txHash: String) {
    chain.faults = chain.faults.copy(unreadableOutcomes = chain.faults.unreadableOutcomes + txHash)
}

fun DurabilityHarness.makeChainUnreachable() {
    chain.faults = chain.faults.copy(pinFails = true)
}

fun DurabilityHarness.makeChainReachable() {
    chain.faults = chain.faults.copy(pinFails = false)
}

// ---- submitting and reading ---------------------------------------------------------------------------

/**
 * One coin in, one coin out. Returns the Result, so a scenario can assert on a refusal.
 *
 * [periodBlocks] is the mortality window. The default is long enough that a hand-written scenario closes it
 * deliberately or not at all; a fuzz walk wants a short one, so windows open and close as it runs.
 */
suspend fun DurabilityHarness.register(
    inputCoin: Int,
    outputCoin: Int,
    periodBlocks: Int = HARNESS_MORTAL_PERIOD,
) = service.submitTransaction(
    extrinsic = extrinsicAnchoredAtFinalizedHead(periodBlocks),
    inputs = listOf(CoinageInput.Coin.Own(testKey(inputCoin))),
    outputs = listOf(OwnAsset.Coin(testKey(outputCoin))),
    groupId = null,
)

/**
 * Several one-coin-in, one-coin-out transactions as one group, each with its own extrinsic.
 *
 * [coinPairs] is input-to-output per transaction, so a scenario can make two of them collide on purpose.
 */
suspend fun DurabilityHarness.registerGroup(
    vararg coinPairs: Pair<Int, Int>,
    groupId: CoinageOperationGroupId = CoinageOperationGroupId("group"),
    periodBlocks: Int = HARNESS_MORTAL_PERIOD,
) = service.submitTransactions(
    transactions = coinPairs.map { (input, output) ->
        CoinageTransactionRequest(
            extrinsic = extrinsicAnchoredAtFinalizedHead(periodBlocks),
            inputs = listOf(CoinageInput.Coin.Own(testKey(input))),
            outputs = listOf(OwnAsset.Coin(testKey(output))),
            policy = null,
        )
    },
    groupId = groupId,
)

/**
 * A coin in and nothing trackable out — the offboard shape.
 *
 * These are the operations Rules 5 and 6 exist for: with no output to look for, the only evidence the entry
 * ran is its own input being gone.
 */
suspend fun DurabilityHarness.givenUnwatchedOffboard(inputCoin: Int): CoinageTransactionId {
    val id = service.submitTransaction(
        extrinsic = extrinsicAnchoredAtFinalizedHead(),
        inputs = listOf(CoinageInput.Coin.Own(testKey(inputCoin))),
        outputs = emptyList(),
        groupId = null,
    ).getOrThrow()
    releaseSubmissions()

    return id
}

/** One voucher in, one coin out — the unload shape. */
suspend fun DurabilityHarness.registerVoucherUnload(
    voucher: Int,
    outputCoin: Int,
    periodBlocks: Int = HARNESS_MORTAL_PERIOD,
) = registerVoucherUnload(vouchers = listOf(voucher), outputCoin = outputCoin, periodBlocks = periodBlocks)

/** Several vouchers of one ring in, one coin out — `unload_recycler_into_coins`. */
suspend fun DurabilityHarness.registerVoucherUnload(
    vouchers: List<Int>,
    outputCoin: Int,
    periodBlocks: Int = HARNESS_MORTAL_PERIOD,
) = service.submitTransaction(
    extrinsic = extrinsicAnchoredAtFinalizedHead(periodBlocks),
    inputs = vouchers.map({ CoinageInput.Voucher(testKey(it)) }),
    outputs = listOf(OwnAsset.Coin(testKey(outputCoin))),
    groupId = null,
)

/** One coin in and nothing trackable out, returning the Result — the offboard shape. */
suspend fun DurabilityHarness.registerOffboard(
    inputCoin: Int,
    periodBlocks: Int = HARNESS_MORTAL_PERIOD,
) = service.submitTransaction(
    extrinsic = extrinsicAnchoredAtFinalizedHead(periodBlocks),
    inputs = listOf(CoinageInput.Coin.Own(testKey(inputCoin))),
    outputs = emptyList(),
    groupId = null,
)

/** One coin in, several coins out — the split shape. */
suspend fun DurabilityHarness.registerSplit(
    inputCoin: Int,
    vararg outputCoins: Int,
    periodBlocks: Int = HARNESS_MORTAL_PERIOD,
) = service.submitTransaction(
    extrinsic = extrinsicAnchoredAtFinalizedHead(periodBlocks),
    inputs = listOf(CoinageInput.Coin.Own(testKey(inputCoin))),
    outputs = outputCoins.map { OwnAsset.Coin(testKey(it)) },
    groupId = null,
)

/**
 * Nothing in, one voucher out — a recycler loaded with an external asset.
 *
 * The user's own money from outside the system becomes a voucher, so the entry has no input at all. It is
 * the mirror of an offboard, and the only shape with an empty input list.
 */
suspend fun DurabilityHarness.registerExternalVoucherLoad(
    voucher: Int,
    periodBlocks: Int = HARNESS_MORTAL_PERIOD,
) = service.submitTransaction(
    extrinsic = extrinsicAnchoredAtFinalizedHead(periodBlocks),
    inputs = emptyList(),
    outputs = listOf(OwnAsset.Voucher(testKey(voucher))),
    groupId = null,
)

/** One coin in, one voucher out — loading a recycler, which is the only way a voucher comes into being. */
suspend fun DurabilityHarness.registerVoucherMint(
    inputCoin: Int,
    voucher: Int,
    periodBlocks: Int = HARNESS_MORTAL_PERIOD,
) = service.submitTransaction(
    extrinsic = extrinsicAnchoredAtFinalizedHead(periodBlocks),
    inputs = listOf(CoinageInput.Coin.Own(testKey(inputCoin))),
    outputs = listOf(OwnAsset.Voucher(testKey(voucher))),
    groupId = null,
)

suspend fun DurabilityHarness.statusOf(id: CoinageTransactionId) = repository.getStatus(id).getOrThrow()

suspend fun DurabilityHarness.assetStateOf(coin: Int) = repository.getAssetState(OwnAsset.Coin(testKey(coin))).getOrThrow()

suspend fun DurabilityHarness.voucherStateOf(voucher: Int) =
    repository.getAssetState(OwnAsset.Voucher(testKey(voucher))).getOrThrow()

/** What the rules would read for [id] against a view pinned now. */
suspend fun DurabilityHarness.evidenceFor(id: CoinageTransactionId): ChainEvidence {
    val collector = CoinageEvidenceCollector(
        voucherRingDerivation = voucherDerivation,
        coinageSigningContextProvider = RealCoinageSigningContextProvider(),
    )

    val view = chain.pin(HARNESS_CHAIN).getOrThrow()

    return collector.collect(repository.getEntry(id).getOrThrow()!!, chain.create(view), view)
}

// ---- internals ----------------------------------------------------------------------------------------

private fun DurabilityHarness.produceBlock(
    finality: TestActionFinality,
    body: List<String> = emptyList(),
    mutate: (CoinageChainState) -> CoinageChainState = { it },
) = chain.produceBlock(body, mutate).also { if (finality == FINALIZED) finalizeToBest() }

internal fun includedPosition(ring: Int) = RingPosition.Included(
    ringIndex = RingIndex(BigInteger.valueOf(ring.toLong())),
    ringPage = 0,
    ringPosition = 0,
)

internal fun onboardingPosition() = RingPosition.Onboarding(queuePage = 0, queuedAt = 0)

/** The alias key the collector will ask for, derived from the chain exactly as it derives it. */
internal suspend fun DurabilityHarness.currentAliasKeyOf(voucher: Int): RecyclerAliasKey? {
    val member = voucherDerivation.memberKeyOf(testKey(voucher))
    val state = chain.chain.bestHead.state
    val denomination = state.recyclerMembers[member] ?: return null
    val ringIndex = (state.ringPositions[member] as? RingPosition.Included)?.ringIndex ?: return null

    return aliasKeyAt(voucher, denomination.value, ringIndex.value.toInt())
}

private suspend fun DurabilityHarness.requireAliasKeyOf(voucher: Int) = currentAliasKeyOf(voucher)
    ?: error("voucher $voucher is in no ring on chain, so it has no alias key")

internal suspend fun DurabilityHarness.aliasKeyAt(voucher: Int, denomination: Int, ring: Int): RecyclerAliasKey {
    val alias = voucherDerivation.aliasWithoutCounting(testKey(voucher), RealCoinageSigningContextProvider().recyclerVouchersContext())

    return RecyclerAliasKey(
        valueExponent = denomination.toBigInteger(),
        recyclerIndex = BigInteger.valueOf(ring.toLong()),
        alias = alias.value.toDataByteArray(),
    )
}
