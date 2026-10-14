package io.paritytech.polkadotapp.database.model

class ChatMessageSearchRow(
    val id: String,
    val chatId: ByteArray,
    val timestamp: Long,
    val searchableContent: String,
)
