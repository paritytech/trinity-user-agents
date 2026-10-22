package io.paritytech.polkadotapp.feature_chats_impl.data.resources

/** The opened HOP files, kept in the files of the app: the node drops a file once it is acked. */
class ChatHopFileStore(val cache: ChatResourceDiskCache) {
    companion object {
        const val MAX_BYTES: Long = 512L * 1024 * 1024
    }
}
