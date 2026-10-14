package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "chat_requests")
class ChatRequestLocal(
    @PrimaryKey val id: String,
    val timestamp: Long,
    val direction: Direction,
    val status: Status
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
}
