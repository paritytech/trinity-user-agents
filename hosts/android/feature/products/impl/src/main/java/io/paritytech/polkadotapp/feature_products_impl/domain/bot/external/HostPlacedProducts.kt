package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import android.content.Context
import androidx.annotation.StringRes
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * The products the host itself puts in chat: listed on first run, with no add, install or discover
 * step, and removable by nobody. The chat analogue of Pocket's privileged card
 * ([io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PinnedPocketCards]).
 *
 * Today that is Jollity, backed by the governance-reserved game product of whichever network the
 * app is on. Placing one grants no permissions; it still prompts like any other product.
 */
@Singleton
class HostPlacedProducts @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    private class Definition(
        @StringRes val nameRes: Int,
        val backingProduct: (DotNsTld) -> ProductId,
    )

    private val definitions = listOf(
        Definition(RCommon.string.host_placed_chat_jollity, ReservedProductIds::game),
    )

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

    private fun products(tld: DotNsTld) = definitions.map {
        Product(it.backingProduct(tld), context.getString(it.nameRes), icon = null)
    }
}
