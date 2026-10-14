package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "chat_rooms")
class ChatRoomLocal(
    @PrimaryKey val id: ByteArray,
    val createdAt: Long,
    val name: String?,
    val icon: String?,
)
