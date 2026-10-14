package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.CoinageInstallationLocal

@Dao
abstract class CoinageInstallationDao {
    @Query("SELECT * FROM coinage_installations WHERE isCurrent = 1 LIMIT 1")
    abstract suspend fun getCurrent(): CoinageInstallationLocal?

    @Insert(onConflict = OnConflictStrategy.ABORT)
    abstract suspend fun insert(installation: CoinageInstallationLocal)

    // The read and the insert share one write transaction, so concurrent first callers on a fresh install all
    // end up with the same id — two current installations would split one device's keys across two subtrees.
    @Transaction
    open suspend fun getOrCreateCurrent(newInstallationId: () -> ByteArray): CoinageInstallationLocal {
        getCurrent()?.let { return it }

        val created = CoinageInstallationLocal(
            installationId = newInstallationId(),
            isCurrent = true,
            coinScanNextIndex = 0,
            voucherScanNextIndex = 0,
            initialScanCompleted = true,
        )
        insert(created)

        return created
    }

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    abstract suspend fun insertIfAbsent(installations: List<CoinageInstallationLocal>)

    @Query("SELECT * FROM coinage_installations WHERE isCurrent = 0")
    abstract suspend fun getPrevious(): List<CoinageInstallationLocal>

    @Query("UPDATE coinage_installations SET coinScanNextIndex = :nextIndex WHERE installationId = :installationId")
    abstract suspend fun updateCoinScanNextIndex(installationId: ByteArray, nextIndex: Int)

    @Query("UPDATE coinage_installations SET voucherScanNextIndex = :nextIndex WHERE installationId = :installationId")
    abstract suspend fun updateVoucherScanNextIndex(installationId: ByteArray, nextIndex: Int)

    @Query("UPDATE coinage_installations SET initialScanCompleted = 1 WHERE installationId = :installationId")
    abstract suspend fun markInitialScanCompleted(installationId: ByteArray)
}
