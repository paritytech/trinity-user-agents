package io.paritytech.polkadotapp.feature_chats_impl.data.resources

/** The opened sealed files, kept in the files of the app so that they stay after the Bulletin chain drops the parts. */
class ChatSealedFileStore(val cache: ChatResourceDiskCache) {
    companion object {
        const val MAX_BYTES: Long = 512L * 1024 * 1024
    }
}
