package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Upsert
import io.paritytech.polkadotapp.database.model.ContactDeviceLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ContactDeviceDao {
    @Upsert
    abstract suspend fun upsert(device: ContactDeviceLocal)

    @Query("SELECT * FROM contact_devices WHERE contactAccountId = :contactAccountId")
    abstract suspend fun getByContact(contactAccountId: ByteArray): List<ContactDeviceLocal>

    @Query("SELECT * FROM contact_devices WHERE contactAccountId = :contactAccountId")
    abstract fun subscribeByContact(contactAccountId: ByteArray): Flow<List<ContactDeviceLocal>>

    @Query("SELECT * FROM contact_devices")
    abstract fun subscribeAll(): Flow<List<ContactDeviceLocal>>

    @Query(
        """
        DELETE FROM contact_devices
        WHERE contactAccountId = :contactAccountId
          AND statementAccountId = :statementAccountId
        """
    )
    abstract suspend fun delete(contactAccountId: ByteArray, statementAccountId: ByteArray)

    @Query("DELETE FROM contact_devices WHERE contactAccountId = :contactAccountId")
    abstract suspend fun deleteAllForContact(contactAccountId: ByteArray)
}
