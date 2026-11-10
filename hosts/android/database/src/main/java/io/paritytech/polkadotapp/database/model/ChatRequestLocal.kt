package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "chat_requests")
class ChatRequestLocal(
    @PrimaryKey val id: String,
    val timestamp: Long,
    val direction: Direction,
    val status: Status,
    @ColumnInfo(defaultValue = "DELIVERED") val deliveryStatus: DeliveryStatus,
    val deliveredVia: DeliveredVia?,
    val lastDeliveredPeriod: Long?,
) {
    enum class Direction {
        INCOMING,
        OUTGOING
    }

    enum class Status {
        PENDING,
        ACCEPTED,
        DECLINED
    }

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
