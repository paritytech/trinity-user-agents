package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.PrimaryKey

@Entity(
    tableName = "chat_seen_states",
    foreignKeys = [
        ForeignKey(
            entity = ContactLocal::class,
            parentColumns = ["accountId"],
            childColumns = ["contactAccountId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
)
class ChatSeenStateLocal(
    @PrimaryKey val contactAccountId: ByteArray,
    val peerSeenOrder: Long,
    val lastSentSeenMessageId: String?,
    val lastSentSeenOrder: Long,
    val lastSentSeenTimestamp: Long,
)
