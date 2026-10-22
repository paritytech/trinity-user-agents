package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

/**
 * What a product declared for one of its rooms (Chat Extension v2), keyed by the chat id of the room.
 * Each blob is the SCALE value of its declaration, as the chats feature encodes it. A null blob means
 * that the product declared nothing for that part. [contact] is the person that a room of the chat product
 * is a conversation with.
 */
@Entity(tableName = "chat_room_declarations")
class ChatRoomDeclarationsLocal(
    @PrimaryKey val chatId: ByteArray,
    val config: ByteArray?,
    @ColumnInfo(defaultValue = "0") val configGeneration: Long,
    val description: String?,
    val preview: ByteArray?,
    val footer: ByteArray?,
    val features: ByteArray?,
    val contact: ByteArray?,
)
