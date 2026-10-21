package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel

/** The bytes of a HOP file and the pool entries that the node drops once the app keeps them. */
class ChatHopFile(val bytes: ByteArray, val entries: List<ByteArray>)

/** Opens the HOP files of the chat product's rooms through the core, which claims them only from the app's HOP nodes. */
interface ChatHopFiles {
    /** The bytes of [resource]. A failure means the file is not available now; a later call tries again. */
    suspend fun open(resource: ChatResourceModel.Hop): Result<ChatHopFile>

    /** Tells the node that the [entries] of an opened file were taken. Call it once the bytes are kept. */
    suspend fun ack(resource: ChatResourceModel.Hop, entries: List<ByteArray>): Result<Unit>
}
