package io.paritytech.polkadotapp.feature_chats_api.domain.search

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.search.ChatListSearchResult

interface ChatSearchResultProvider {
    /**
     * Namespace this provider's results belong to, stamped onto every [ChatListSearchResult.App]
     * it produces so a selected result routes back to its producer without any ownership probing.
     */
    val id: ChatExtensionId

    suspend fun search(query: String): Result<List<ChatListSearchResult.App>>

    suspend fun onAppResultSelected(result: ChatListSearchResult.App)
}
