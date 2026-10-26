package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Upsert
import io.paritytech.polkadotapp.database.model.ChatAcceptanceLocal
import io.paritytech.polkadotapp.database.model.ChatAcceptanceLocal.DeliveredVia
import io.paritytech.polkadotapp.database.model.ChatAcceptanceLocal.DeliveryStatus
import kotlinx.coroutines.flow.Flow

@Dao
interface ChatAcceptanceDao {
    @Upsert
    suspend fun upsert(acceptance: ChatAcceptanceLocal)

    @Query("SELECT * FROM chat_acceptances WHERE contactAccountId = :contactAccountId")
    suspend fun get(contactAccountId: ByteArray): ChatAcceptanceLocal?

    @Query(
        """
        SELECT chat_acceptances.* FROM chat_acceptances
        INNER JOIN contacts ON contacts.accountId = chat_acceptances.contactAccountId
        WHERE chat_acceptances.deliveryStatus = 'UNDELIVERED' AND contacts.isBlocked = 0
        """
    )
    fun subscribeUndelivered(): Flow<List<ChatAcceptanceLocal>>

    @Query(
        """
        SELECT chat_acceptances.* FROM chat_acceptances
        INNER JOIN contacts ON contacts.accountId = chat_acceptances.contactAccountId
        WHERE chat_acceptances.deliveryStatus = 'DELIVERED' AND contacts.isBlocked = 0
        """
    )
    suspend fun getDelivered(): List<ChatAcceptanceLocal>

    @Query(
        """
        UPDATE chat_acceptances
        SET deliveryStatus = :deliveryStatus, deliveredVia = :deliveredVia, lastDeliveredPeriod = :lastDeliveredPeriod
        WHERE contactAccountId = :contactAccountId AND messageId = :messageId
        """
    )
    suspend fun updateDelivery(
        contactAccountId: ByteArray,
        messageId: String,
        deliveryStatus: DeliveryStatus,
        deliveredVia: DeliveredVia?,
        lastDeliveredPeriod: Long?,
    )

    @Query("DELETE FROM chat_acceptances WHERE contactAccountId = :contactAccountId")
    suspend fun delete(contactAccountId: ByteArray)
}
