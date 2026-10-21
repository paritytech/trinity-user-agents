package io.paritytech.polkadotapp.database.model

import androidx.room.Entity

/**
 * A bot persona of a chat extension (Chat Extension v2 additions): a name and an icon that the extension
 * speaks as in its own rooms, with the participant id `bot:<botId>`. [icon] is the address that the
 * product gave (`https` or `data:`).
 */
@Entity(
    tableName = "chat_bots",
    primaryKeys = ["extensionId", "botId"],
)
class ChatBotPersonaLocal(
    val extensionId: String,
    val botId: String,
    val name: String,
    val icon: String,
)
