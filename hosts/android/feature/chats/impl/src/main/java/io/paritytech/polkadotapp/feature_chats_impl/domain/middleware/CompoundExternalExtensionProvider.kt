package io.paritytech.polkadotapp.feature_chats_impl.domain.middleware

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOf
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class CompoundExternalExtensionProvider @Inject constructor(
    private val providers: Set<@JvmSuppressWildcards ExternalExtensionProvider>,
) : ExternalExtensionProvider {
    override fun observeExtensions(): Flow<List<ExternalExtension>> {
        if (providers.isEmpty()) {
            return flowOf(emptyList())
        }

        val flows = providers.map { it.observeExtensions() }
        return combine(flows) { arrays ->
            arrays.flatMap { it.toList() }
        }
    }
}
