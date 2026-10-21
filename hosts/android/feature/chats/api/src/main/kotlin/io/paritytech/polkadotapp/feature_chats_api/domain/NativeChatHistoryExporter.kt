package io.paritytech.polkadotapp.feature_chats_api.domain

/**
 * The native chat's history, as the chat product imports it when the app moves this install's chat over to it (HX3,
 * P16): the SCALE `HistoryExport::V1` of the contacts, their pending requests and the stored messages.
 */
interface NativeChatHistoryExporter {
    /** Null when the native chat has no contact. */
    suspend fun export(): ByteArray?
}
