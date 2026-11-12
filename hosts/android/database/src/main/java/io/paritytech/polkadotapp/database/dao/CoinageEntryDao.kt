package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.CoinageAssetKindLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryInputLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryOutputLocal
import io.paritytech.polkadotapp.database.model.CoinageHandoffLocal
import io.paritytech.polkadotapp.database.model.DurableTxLocal
import kotlinx.coroutines.flow.Flow

/** Coinage's rows of the shared ledger. The transaction row itself belongs to [DurableTxDao]. */
private const val COINAGE_DOMAIN = "coinage"

/**
 * Every asset this subsystem knows anything about, with its lock and the status of the transaction minting
 * it.
 *
 * The key space is the union of everything that can carry either fact; each fact is then a correlated
 * lookup, which keeps this one query rather than three that could observe different snapshots.
 */
private const val ASSET_STATE_QUERY = """
    SELECT k.assetKind AS assetKind,
           k.installationId AS installationId,
           k.derivationIndex AS derivationIndex,
           (
               SELECT e.status FROM coinage_entry_output o
               JOIN durable_tx e ON e.id = o.entryId
               WHERE o.assetKind = k.assetKind AND o.installationId = k.installationId
                 AND o.derivationIndex = k.derivationIndex
               LIMIT 1
           ) AS minterStatus,
           EXISTS(
               SELECT 1 FROM coinage_handoff h
               WHERE h.assetKind = k.assetKind AND h.installationId = k.installationId
                 AND h.derivationIndex = k.derivationIndex
           ) AS handedOff,
           (
               SELECT e2.status FROM coinage_entry_input i
               JOIN durable_tx e2 ON e2.id = i.entryId
               WHERE i.assetKind = k.assetKind AND i.installationId = k.installationId
                 AND i.derivationIndex = k.derivationIndex
                 AND e2.status != 'FAILURE'
               LIMIT 1
           ) AS consumerStatus
    FROM (
        SELECT assetKind, installationId, derivationIndex FROM coinage_entry_output
        UNION
        SELECT assetKind, installationId, derivationIndex FROM coinage_entry_input WHERE derivationIndex IS NOT NULL
        UNION
        SELECT assetKind, installationId, derivationIndex FROM coinage_handoff
    ) k
"""

@Dao
abstract class CoinageEntryDao {
    /**
     * Runs [action] in one database transaction.
     *
     * Registration does not need this — it runs inside the engine's transaction already — but a handoff
     * writes coinage rows alone and still has to check and mark atomically.
     */
    @Transaction
    open suspend fun withTransaction(action: suspend () -> Unit) {
        return action()
    }

    @Insert(onConflict = OnConflictStrategy.ABORT)
    abstract suspend fun insertInputs(inputs: List<CoinageEntryInputLocal>)

    @Insert(onConflict = OnConflictStrategy.ABORT)
    abstract suspend fun insertOutputs(outputs: List<CoinageEntryOutputLocal>)

    /**
     * A conflict here can only be the same key twice in one batch, because a key already marked is rejected
     * before the insert; that is the same mark, and the row it collapses to is the one the mark means.
     */
    @Insert(onConflict = OnConflictStrategy.IGNORE)
    abstract suspend fun insertHandoffs(handoffs: List<CoinageHandoffLocal>)

    // ---- invariant support, one batched query each ----

    /** Of [onChainKeys], those already minted by some transaction. */
    @Query("SELECT onChainKey FROM coinage_entry_output WHERE onChainKey IN (:onChainKeys)")
    abstract suspend fun filterMinted(onChainKeys: List<ByteArray>): List<ByteArray>

    /** Of [onChainKeys], those that are a key a peer sent us. */
    @Query(
        """
        SELECT onChainKey FROM coinage_entry_input
        WHERE onChainKey IN (:onChainKeys) AND derivationIndex IS NULL
        """
    )
    abstract suspend fun filterReceivedKeys(onChainKeys: List<ByteArray>): List<ByteArray>

    /** Of [onChainKeys], those already claimed by a transaction that has not failed. */
    @Query(
        """
        SELECT i.onChainKey FROM coinage_entry_input i
        JOIN durable_tx e ON e.id = i.entryId
        WHERE i.onChainKey IN (:onChainKeys) AND e.status != 'FAILURE'
        """
    )
    abstract suspend fun filterClaimed(onChainKeys: List<ByteArray>): List<ByteArray>

    /** Of [onChainKeys], those whose key has left the device. */
    @Query("SELECT onChainKey FROM coinage_handoff WHERE onChainKey IN (:onChainKeys)")
    abstract suspend fun filterHandedOff(onChainKeys: List<ByteArray>): List<ByteArray>

    // ---- assets ----

    @Query("SELECT * FROM coinage_entry_input WHERE entryId IN (:entryIds)")
    abstract suspend fun inputsOf(entryIds: List<Long>): List<CoinageEntryInputLocal>

    @Query("SELECT * FROM coinage_entry_output WHERE entryId IN (:entryIds)")
    abstract suspend fun outputsOf(entryIds: List<Long>): List<CoinageEntryOutputLocal>

    @Query("SELECT * FROM coinage_handoff")
    abstract suspend fun getHandoffs(): List<CoinageHandoffLocal>

    @Query("UPDATE coinage_handoff SET committed = 1 WHERE onChainKey IN (:onChainKeys)")
    abstract suspend fun commitHandoffs(onChainKeys: List<ByteArray>)

    /**
     * A provisional mark belongs to a payment that never became durable, so its keys never left. Clearing it
     * on launch is what returns those assets; a committed mark is never touched.
     */
    @Query("DELETE FROM coinage_handoff WHERE committed = 0")
    abstract suspend fun deleteUncommittedHandoffs()

    @Query("DELETE FROM coinage_handoff WHERE committed = 0 AND onChainKey IN (:onChainKeys)")
    abstract suspend fun deleteUncommittedHandoffs(onChainKeys: List<ByteArray>)

    // ---- derived views ----

    @Transaction
    @Query(
        """
        SELECT * FROM durable_tx
        WHERE domainId = '$COINAGE_DOMAIN' AND operationGroupId = :groupId
        ORDER BY id ASC
        """
    )
    abstract suspend fun getGroupEntries(groupId: String): List<CoinageEntryWithAssets>

    @Transaction
    @Query(
        """
        SELECT * FROM durable_tx
        WHERE domainId = '$COINAGE_DOMAIN' AND operationGroupId = :groupId
        ORDER BY id ASC
        """
    )
    abstract fun subscribeGroupEntries(groupId: String): Flow<List<CoinageEntryWithAssets>>

    @Query(ASSET_STATE_QUERY)
    abstract fun subscribeAssetStates(): Flow<List<CoinageAssetStateProjection>>

    @Query(
        "$ASSET_STATE_QUERY WHERE k.assetKind = :assetKind AND k.installationId = :installationId " +
            "AND k.derivationIndex = :derivationIndex"
    )
    abstract suspend fun getAssetState(
        assetKind: CoinageAssetKindLocal,
        installationId: ByteArray,
        derivationIndex: Int,
    ): CoinageAssetStateProjection?

    // One kind and installation at a time: SQLite has no tuple IN.
    @Query(
        "$ASSET_STATE_QUERY WHERE k.assetKind = :assetKind AND k.installationId = :installationId " +
            "AND k.derivationIndex IN (:derivationIndices)"
    )
    abstract suspend fun getAssetStates(
        assetKind: CoinageAssetKindLocal,
        installationId: ByteArray,
        derivationIndices: List<Int>,
    ): List<CoinageAssetStateProjection>
}

/** A transaction with Coinage's assets, fetched in one query rather than three. */
class CoinageEntryWithAssets(
    @androidx.room.Embedded val entry: DurableTxLocal,
    @androidx.room.Relation(parentColumn = "id", entityColumn = "entryId")
    val inputs: List<CoinageEntryInputLocal>,
    @androidx.room.Relation(parentColumn = "id", entityColumn = "entryId")
    val outputs: List<CoinageEntryOutputLocal>,
)

class CoinageAssetStateProjection(
    val assetKind: CoinageAssetKindLocal,
    val installationId: ByteArray,
    val derivationIndex: Int,
    val minterStatus: DurableTxLocal.Status?,
    val handedOff: Boolean,
    /** Of the at-most-one non-failed consumer — the Unique consumer invariant is what makes LIMIT 1 sound. */
    val consumerStatus: DurableTxLocal.Status?,
)
