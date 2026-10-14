package io.paritytech.polkadotapp.feature_coinage_impl.data.transaction

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import java.math.BigInteger

/** The `RecyclerAliasStates` key: value exponent, recycler index, and the voucher's derived alias. */
data class RecyclerAliasKey(
    val valueExponent: BigInteger,
    val recyclerIndex: BigInteger,
    val alias: DataByteArray,
)

/**
 * Coinage's reads of the chain at one pinned view.
 *
 * Every batched read keeps one shape, so a caller never has to remember which map omits what:
 *
 * - `Result.failure` — the read failed. Nothing about any requested key is known.
 * - a successful map — **every requested key is present**. A `null` value is the chain holding no value for
 *   it, which is a real answer.
 *
 * The storage layer drops keys with no value from its response, so implementations must put them back.
 * Callers look up with `getValue`, which turns a violation of this into an exception rather than a verdict.
 */
interface CoinageStateReader {
    suspend fun coinsAt(at: BlockHash, coins: List<AccountId>): Result<Map<AccountId, OnChainCoinInfo?>>

    /**
     * The denomination of the recycler each voucher is a member of, null when it is in none. That is not a
     * voucher that is gone — archival removes this entry while the voucher stays redeemable — so it reads as
     * unknown, and it is also why the collection id cannot be built for that voucher.
     */
    suspend fun recyclerMembershipsAt(
        at: BlockHash,
        memberKeys: List<BandersnatchPublicKey>,
    ): Result<Map<BandersnatchPublicKey, ValueExponent?>>

    /** Where each voucher of [memberships] sits in its collection at [at]; null when it is not a member there. */
    suspend fun ringPositionsAt(
        at: BlockHash,
        memberships: Map<BandersnatchPublicKey, ValueExponent>,
    ): Result<Map<BandersnatchPublicKey, RingPosition?>>

    suspend fun aliasStatesAt(
        at: BlockHash,
        keys: List<RecyclerAliasKey>,
    ): Result<Map<RecyclerAliasKey, OnChainAliasState?>>
}

/** Builds a reader bound to the block hashes the engine has already pinned. */
interface CoinageStateReaderFactory {
    suspend fun create(view: PinnedChainView): CoinageStateReader
}
