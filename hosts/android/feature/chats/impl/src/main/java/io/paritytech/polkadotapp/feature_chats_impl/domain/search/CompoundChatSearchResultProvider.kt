package io.paritytech.polkadotapp.feature_chats_impl.domain.search

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.search.ChatListSearchResult
import io.paritytech.polkadotapp.feature_chats_api.domain.search.ChatSearchResultProvider
import timber.log.Timber
import javax.inject.Inject

class CompoundChatSearchResultProvider @Inject constructor(
    private val providers: Set<@JvmSuppressWildcards ChatSearchResultProvider>,
) {
    private val providersById = providers.associateBy { it.id }

    suspend fun search(query: String): List<ChatListSearchResult.App> {
        return providers.flatMap { provider ->
            provider.search(query)
                .logFailure("ChatSearchResultProvider.search failed")
                .getOrElse { emptyList() }
        }
    }

    suspend fun onResultSelected(result: ChatListSearchResult.App) {
        val provider = providersById[result.providerId]

        if (provider == null) {
            Timber.w("No provider ${result.providerId} for app result ${result.id}")
            return
        }

        provider.onAppResultSelected(result)
    }
}
