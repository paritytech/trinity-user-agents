package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ExternalPaymentLocal
import kotlinx.coroutines.flow.Flow
import java.math.BigInteger

@Dao
interface ExternalPaymentDao {
    @Insert(onConflict = OnConflictStrategy.ABORT)
    suspend fun insert(payment: ExternalPaymentLocal)

    @Query("SELECT * FROM external_payments WHERE origin = :origin AND id = :id")
    suspend fun getById(origin: String, id: String): ExternalPaymentLocal?

    @Query("SELECT EXISTS(SELECT 1 FROM external_payments WHERE origin = :origin AND id = :id)")
    suspend fun exists(origin: String, id: String): Boolean

    @Query("SELECT * FROM external_payments WHERE origin = :origin AND id = :id")
    fun observeById(origin: String, id: String): Flow<ExternalPaymentLocal?>

    @Query(
        """
        SELECT * FROM external_payments
        WHERE stage NOT IN ('COMPLETED', 'PARTIALLY_COMPLETED', 'FAILED')
        ORDER BY createdAt ASC LIMIT 1
        """
    )
    suspend fun getNextPending(): ExternalPaymentLocal?

    @Query(
        """
        UPDATE external_payments SET
            stage = :stage,
            selectedVoucherKeys = :selectedVoucherKeys,
            surplusPlanks = :surplusPlanks,
            claimedPlanks = :claimedPlanks,
            failureReason = :failureReason,
            updatedAt = :updatedAt
        WHERE origin = :origin AND id = :id
        """
    )
    suspend fun updateStage(
        origin: String,
        id: String,
        stage: ExternalPaymentLocal.Stage,
        selectedVoucherKeys: String?,
        surplusPlanks: BigInteger?,
        claimedPlanks: BigInteger?,
        failureReason: String?,
        updatedAt: Long,
    )
}
