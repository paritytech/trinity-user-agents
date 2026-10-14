package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.database.model.CoinLocal
import kotlinx.coroutines.flow.Flow

/** Coins the chain holds right now — presence, not the age it was last seen with. */
private const val ON_CHAIN_COINS_QUERY = "SELECT * FROM coins WHERE onChain = 1"

@Dao
interface CoinDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(coin: CoinLocal)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertAll(coins: List<CoinLocal>)

    // A freshly allocated key must never land on an existing row: that row's key may already be handed off.
    @Insert(onConflict = OnConflictStrategy.ABORT)
    suspend fun insertNew(coins: List<CoinLocal>)

    @Query("SELECT * FROM coins")
    fun subscribeAll(): Flow<List<CoinLocal>>

    @Query("SELECT * FROM coins")
    suspend fun getAll(): List<CoinLocal>

    @Query("SELECT * FROM coins WHERE accountId IN (:accountIds)")
    fun subscribeBy(accountIds: List<ByteArray>): Flow<List<CoinLocal>>

    @Query("SELECT * FROM coins WHERE installationId = :installationId AND derivationIndex IN (:derivationIndices)")
    suspend fun getByDerivationIndices(installationId: ByteArray, derivationIndices: List<Int>): List<CoinLocal>

    @Query("SELECT * FROM coins WHERE ageValue IS NULL")
    fun subscribeAllCoinsWithUnknownAge(): Flow<List<CoinLocal>>

    @Query("SELECT MAX(derivationIndex) FROM coins WHERE installationId = :installationId")
    suspend fun getMaxDerivationIndex(installationId: ByteArray): Int?

    /**
     * Presence always; the age only when the chain gave one.
     *
     * COALESCE is what keeps the age monotonic: a coin that has left the chain keeps the last age it was
     * seen with, so "never observed" stays distinguishable from "observed, now gone".
     */
    @Query("UPDATE coins SET onChain = :onChain, ageValue = COALESCE(:age, ageValue) WHERE accountId = :accountId")
    suspend fun updateCoinPresence(accountId: ByteArray, onChain: Boolean, age: Int?)

    @Transaction
    suspend fun updateCoins(updates: List<CoinUpdateLocal>) {
        updates.forEach { updateCoinPresence(accountId = it.accountId.value, onChain = it.onChain, age = it.age) }
    }

    @Query(ON_CHAIN_COINS_QUERY)
    suspend fun getOnChainCoins(): List<CoinLocal>

    @Query(ON_CHAIN_COINS_QUERY)
    fun subscribeOnChainCoins(): Flow<List<CoinLocal>>

    @Query("SELECT * FROM coins WHERE onChain = 1 AND ageValue >= :minAge")
    suspend fun getCoinsWithKnownAgeAtLeast(minAge: Int): List<CoinLocal>

    /**
     * Kept apart from [updateCoinPresence] because the two run at completely different rates: presence is
     * rewritten on every chain tick, while hops are filled in once, when a claimed coin's age first arrives.
     */
    @Query(
        "UPDATE coins SET hops = :hops WHERE installationId = :installationId AND derivationIndex = :derivationIndex"
    )
    suspend fun updateCoinHops(installationId: ByteArray, derivationIndex: Int, hops: ByteArray)

    @Transaction
    suspend fun updateCoinHops(updates: List<CoinHopsUpdateLocal>) {
        updates.forEach {
            updateCoinHops(installationId = it.installationId, derivationIndex = it.derivationIndex, hops = it.hops)
        }
    }
}

class CoinHopsUpdateLocal(
    val installationId: ByteArray,
    val derivationIndex: Int,
    /** SCALE-encoded `Vec<CoinHopLocal>`. */
    val hops: ByteArray,
)

class CoinUpdateLocal(
    val accountId: AccountId,
    val onChain: Boolean,
    /** Null when the chain gave no age, which leaves the last one known standing. */
    val age: Int?,
)
