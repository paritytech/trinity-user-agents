package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.Product
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import uniffi.truapi.hostPlacedChats
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The products the host itself puts in chat: listed on first run, with no add, install or discover
 * step, and removable by nobody. The chat analogue of Pocket's privileged card
 * ([io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PinnedPocketCards]).
 *
 * The core decides which they are, on whichever network the app is on.
 */
@Singleton
class HostPlacedProducts @Inject constructor(
    private val dotNsTldProvider: DotNsTldProvider,
) {
    /**
     * Nothing until the TLD is known, so the chats a device already has are not held back by a
     * chain read; a TLD persisted by a previous process costs none.
     */
    fun observe(): Flow<List<Product>> = flow {
        val tld = dotNsTldProvider.currentTldOrNull() ?: run {
            emit(emptyList())
            dotNsTldProvider.getTldRetrying()
        }
        emit(products(tld))
    }

    private fun products(tld: DotNsTld) = hostPlacedChats(tld.value).map {
        Product(it.productId, it.name, icon = null)
    }
}
