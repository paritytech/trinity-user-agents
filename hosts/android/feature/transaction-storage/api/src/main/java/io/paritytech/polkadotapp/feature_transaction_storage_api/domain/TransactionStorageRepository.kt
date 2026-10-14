package io.paritytech.polkadotapp.feature_transaction_storage_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.TransactionStorageAuthorization
import kotlinx.coroutines.flow.Flow

interface TransactionStorageRepository {
    /**
     * Single read of the transaction-storage authorization for [accountId]. Pass
     * [CacheableDataConsistency.CONSISTENT_WITH_REMOTE] when the freshest state matters
     * (e.g. sponsoring decisions) or [CacheableDataConsistency.CAN_BE_STALE] to allow a
     * locally-cached value (e.g. UI displays).
     */
    suspend fun getAuthorization(
        chainId: ChainId,
        accountId: AccountId,
        consistency: CacheableDataConsistency,
    ): Result<TransactionStorageAuthorization?>

    /**
     * Live flow of the authorization for [accountId]. With
     * [CacheableDataConsistency.CAN_BE_STALE] the flow reads from the local-cache feed
     * driven by a runtime updater (efficient for accounts already syncing); with
     * [CacheableDataConsistency.CONSISTENT_WITH_REMOTE] it opens a fresh remote
     * subscription (use this for arbitrary accounts not covered by an updater).
     */
    fun subscribeAuthorization(
        chainId: ChainId,
        accountId: AccountId,
        consistency: CacheableDataConsistency,
    ): Flow<TransactionStorageAuthorization?>

    /**
     * Runtime constant `AuthorizationPeriod` — duration of one authorization window in blocks.
     */
    suspend fun authorizationPeriod(chainId: ChainId): Result<BlockNumber>

    /**
     * Calls `HopRuntimeApi.can_account_promote` on [chainId]. The HOP node applies the same check to
     * `hop_submit`. Returns `true` when the account holds an unexpired `TransactionStorage` authorization.
     */
    suspend fun canAccountPromote(chainId: ChainId, accountId: AccountId): Result<Boolean>

    /**
     * Evaluates [canAccountPromote] once immediately and then on every new [chainId] head.
     */
    fun subscribeCanAccountPromote(chainId: ChainId, accountId: AccountId): Flow<Boolean>
}
