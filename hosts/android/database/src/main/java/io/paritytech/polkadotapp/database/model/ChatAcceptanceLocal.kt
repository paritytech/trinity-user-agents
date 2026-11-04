package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.PrimaryKey

/** An acceptance of a contact's chat request that is re-published until the contact is seen on its device route. */
@Entity(
    tableName = "chat_acceptances",
    foreignKeys = [
        ForeignKey(
            entity = ContactLocal::class,
            parentColumns = ["accountId"],
            childColumns = ["contactAccountId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
)
class ChatAcceptanceLocal(
    @PrimaryKey val contactAccountId: ByteArray,
    val requestId: String,
    val messageId: String,
    val acceptedAt: Long,
    val deliveryStatus: DeliveryStatus,
    val deliveredVia: DeliveredVia?,
    val lastDeliveredPeriod: Long?,
) {
    enum class DeliveryStatus {
        UNDELIVERED,
        DELIVERED,
        FAILED
    }

    enum class DeliveredVia {
        NOTIFICATION,
        USERNAME
    }
}
