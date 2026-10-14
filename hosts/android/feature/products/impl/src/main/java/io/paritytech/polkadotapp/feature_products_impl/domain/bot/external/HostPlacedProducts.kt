package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * A product the host itself puts in chat: present on first run, with no add, install or discover
 * step, and removable by nobody. The chat analogue of Pocket's privileged card
 * ([io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PinnedPocketCards]).
 *
 * Placing one grants no permissions; it still prompts like any other product.
 */
data class HostPlacedProduct(
    /** Names the list's row until the product's own `createRoom` names the room, and the menu's entry always. */
    val product: Product,
    /**
     * Must match the room the product registers, which the host has no way to check. A product
     * that registers another room, or none, gets that chat as well and leaves this one empty.
     */
    val roomId: ProductChatIdParameter,
)

/** Jollity, backed by the governance-reserved game product of whichever network the app is on. */
@Singleton
class HostPlacedProducts @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    /**
     * Nothing until the TLD is known, so the chats a device already has are not held back by a
     * chain read; a TLD persisted by a previous process costs none.
     */
    fun observe(): Flow<List<HostPlacedProduct>> = flow {
        val tld = dotNsTldProvider.currentTldOrNull() ?: run {
            emit(emptyList())
            dotNsTldProvider.getTldRetrying()
        }
        emit(products(tld))
    }

    private fun products(tld: DotNsTld) = listOf(
        HostPlacedProduct(
            product = Product(ReservedProductIds.game(tld), context.getString(RCommon.string.host_placed_chat_jollity), icon = null),
            roomId = ProductChatIdParameter("jollity"),
        ),
    )
}
