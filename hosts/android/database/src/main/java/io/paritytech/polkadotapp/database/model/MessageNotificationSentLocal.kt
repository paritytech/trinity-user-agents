package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(
    tableName = "message_notification_sent"
)
class MessageNotificationSentLocal(
    @PrimaryKey val messageId: String,
    val timestamp: Long
)
