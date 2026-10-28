package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.RingVrfKeyRegistrationLocal
import uniffi.truapi.ProductId

@Dao
abstract class RingVrfKeyRegistrationDao {
    /** Registration is idempotent: the same (owner, index, ring) triple replaces its own row. */
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    abstract suspend fun insert(registration: RingVrfKeyRegistrationLocal)

    @Query("SELECT * FROM ring_vrf_key_registrations WHERE ownerProductId = :ownerProductId")
    abstract suspend fun getByOwner(ownerProductId: ProductId): List<RingVrfKeyRegistrationLocal>

    @Query(
        """
        SELECT * FROM ring_vrf_key_registrations
        WHERE ownerProductId = :ownerProductId AND derivationIndex = :derivationIndex
        """
    )
    abstract suspend fun getByHandle(
        ownerProductId: ProductId,
        derivationIndex: ByteArray
    ): List<RingVrfKeyRegistrationLocal>
}
