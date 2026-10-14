package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.RecyclerAliasKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash

/**
 * What the chain holds at one block: which assets exist, which aliases read as unloaded, and the dispatch
 * outcome of every extrinsic applied in that block.
 *
 * [outcomes] is per-block rather than cumulative, so a transaction reorged out of one block and re-applied
 * in another carries the outcome of the block it is read at.
 */
data class CoinageChainState(
    val coins: Map<AccountId, OnChainCoinInfo>,
    val aliases: Map<RecyclerAliasKey, OnChainAliasState>,
    val outcomes: Map<TransactionHash, ExtrinsicOutcome>,
    /** `RecyclersCoinToRecycler`: which denomination's recycler a voucher belongs to. */
    val recyclerMembers: Map<BandersnatchPublicKey, ValueExponent>,
    /** `Members`: where a voucher sits in that collection. */
    val ringPositions: Map<BandersnatchPublicKey, RingPosition>,
) {
    companion object {
        val EMPTY = CoinageChainState(
            coins = emptyMap(),
            aliases = emptyMap(),
            outcomes = emptyMap(),
            recyclerMembers = emptyMap(),
            ringPositions = emptyMap(),
        )
    }
}

fun CoinageChainState.joinRecycler(member: BandersnatchPublicKey, denomination: ValueExponent, position: RingPosition) =
    copy(recyclerMembers = recyclerMembers + (member to denomination), ringPositions = ringPositions + (member to position))

/** Archival: the member-to-denomination entry goes first and synchronously, before any dusting. */
fun CoinageChainState.leaveRecycler(member: BandersnatchPublicKey) =
    copy(recyclerMembers = recyclerMembers - member, ringPositions = ringPositions - member)

fun CoinageChainState.mintCoin(key: AccountId, value: Int, age: Int): CoinageChainState =
    copy(coins = coins + (key to OnChainCoinInfo(instanceId = 0, value = value, age = age)))

fun CoinageChainState.consumeCoin(key: AccountId): CoinageChainState = copy(coins = coins - key)

fun CoinageChainState.withAlias(key: RecyclerAliasKey, state: OnChainAliasState): CoinageChainState =
    copy(aliases = aliases + (key to state))

fun CoinageChainState.clearAlias(key: RecyclerAliasKey): CoinageChainState = copy(aliases = aliases - key)

fun CoinageChainState.applied(txHash: TransactionHash, outcome: ExtrinsicOutcome): CoinageChainState =
    copy(outcomes = outcomes + (txHash to outcome))

/**
 * Which reads fail, so a scenario can hold an entry undecided without changing what the chain holds.
 *
 * Failures are per-key and per-block rather than global, because the spec distinguishes a pass that reads
 * nothing from one that reads only part of its window.
 */
data class ChainReadFaults(
    val unreadableCoins: Set<AccountId>,
    val unreadableAliases: Set<RecyclerAliasKey>,
    val membershipsUnreadable: Boolean,
    val ringPositionsUnreadable: Boolean,
    val unreadableBlocks: Set<Long>,
    /** A standing rule rather than a set, so it covers blocks produced after it was switched on. */
    val everyBlockUnreadable: Boolean,
    val unreadableOutcomes: Set<TransactionHash>,
    val pinFails: Boolean,
    /**
     * The body search reads nothing, so it can never decide an entry.
     *
     * Separate from [everyBlockUnreadable] because that one also takes out the block reads registration and
     * pinning need, which stops a scenario before it can even start.
     */
    val txSearchDisabled: Boolean,
    /**
     * Coin reads fail at these blocks only.
     *
     * Separate from [unreadableCoins], which fails a key at every head: the rules read the same asset at the
     * finalized and the best head, and some of them turn on the two answers differing.
     */
    val statelessBlocks: Set<String>,
) {
    companion object {
        val NONE = ChainReadFaults(
            unreadableCoins = emptySet(),
            unreadableAliases = emptySet(),
            membershipsUnreadable = false,
            ringPositionsUnreadable = false,
            unreadableBlocks = emptySet(),
            everyBlockUnreadable = false,
            unreadableOutcomes = emptySet(),
            pinFails = false,
            txSearchDisabled = false,
            statelessBlocks = emptySet(),
        )
    }
}

class ChainReadFailure(message: String) : Exception(message)
