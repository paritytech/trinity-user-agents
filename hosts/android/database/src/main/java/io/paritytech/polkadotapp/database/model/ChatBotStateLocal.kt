package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(
    tableName = "chat_bot_state"
)
class ChatBotStateLocal(
    @PrimaryKey val middlewareId: String,
    val isActive: Boolean
)
