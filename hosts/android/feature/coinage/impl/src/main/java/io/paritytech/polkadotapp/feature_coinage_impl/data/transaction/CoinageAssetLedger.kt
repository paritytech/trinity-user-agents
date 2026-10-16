package io.paritytech.polkadotapp.feature_coinage_impl.data.transaction

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetStates
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.RegistrationScope
import kotlinx.coroutines.flow.Flow

/** The assets one transaction consumes and mints, as registration supplies them. */
data class AssetRegistration(
    val inputs: List<RegistrationInput>,
    val outputs: List<RegistrationOutput>,
)

/** The assets one transaction consumes and mints, as the ledger holds them. */
data class EntryAssets(
    val inputs: List<LedgerAsset>,
    val outputs: List<LedgerAsset>,
)

/** Query helpers usable inside the engine's registration transaction. */
interface RegistrationValidationScope {
    /** Of [keys], those already minted by some transaction. */
    suspend fun filterMinted(keys: List<AssetPublicKey>): Set<AssetPublicKey>

    /** Of [keys], those that are a key a peer sent us. */
    suspend fun filterReceived(keys: List<AssetPublicKey>): Set<AssetPublicKey>

    /** Of [keys], those already claimed by a transaction that has not failed. */
    suspend fun filterClaimed(keys: List<AssetPublicKey>): Set<AssetPublicKey>

    /** Of [keys], those whose key has left the device. */
    suspend fun filterHandedOff(keys: List<AssetPublicKey>): Set<AssetPublicKey>
}

/**
 * Coinage's half of the ledger: which assets each transaction consumes and mints, and which carry a lock.
 *
 * The transaction rows themselves belong to the durability engine. These rows key on the id it assigns and
 * are written inside its registration transaction, so the two commit together.
 */
interface CoinageAssetLedger {
    /**
     * Writes the asset rows for [registrations] after checking the invariants they must not break.
     *
     * The [RegistrationScope] context is the point: these rows are only correct inside the engine's write
     * transaction, where throwing rolls the whole registration back. Requiring it makes calling this
     * anywhere else a compile error rather than a silently unprotected write.
     */
    context(_: RegistrationScope)
    suspend fun registerAssets(registrations: List<Pair<CoinageTransactionId, AssetRegistration>>)

    /**
     * Marks [assets] as gone from the device after checking nothing still claims them, in one transaction
     * so nothing can claim them between the check and the mark.
     */
    suspend fun markHandedOff(assets: List<LedgerAsset>): Result<Unit>

    /** Makes the marks on [keys] final. */
    suspend fun commitHandoffs(keys: List<AssetPublicKey>): Result<Unit>

    /** Drops every mark that was never committed — the payments behind them never happened. */
    suspend fun releaseUncommittedHandoffs(): Result<Unit>

    /** Drops the marks on [keys] that were never committed. */
    suspend fun releaseUncommittedHandoffs(keys: List<AssetPublicKey>): Result<Unit>

    suspend fun getHandoffKeys(): Result<Set<AssetPublicKey>>

    /** The assets of each of [ids], in one batched read rather than one per transaction. */
    suspend fun assetsOf(ids: List<CoinageTransactionId>): Result<Map<CoinageTransactionId, EntryAssets>>

    suspend fun getGroupStatuses(groupId: CoinageOperationGroupId): Result<List<CoinageTransactionState>>

    fun subscribeGroupStatuses(groupId: CoinageOperationGroupId): Flow<List<CoinageTransactionState>>

    fun subscribeAssetStates(): Flow<CoinageAssetStates>

    suspend fun getAssetState(asset: OwnAsset): Result<CoinageAssetState>

    suspend fun getAssetStates(assets: List<OwnAsset>): Result<CoinageAssetStates>
}
