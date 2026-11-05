package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import android.content.Context
import androidx.annotation.StringRes
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * The products the host itself puts in chat: listed on first run, with no add, install or discover
 * step, and removable by nobody. The chat analogue of Pocket's privileged card
 * ([io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PinnedPocketCards]).
 *
 * Each is a governance-reserved product of whichever network the app is on, with the one room the
 * host places for it. The Polkadot room is moving from the game's `jollity` to Humanity's
 * `polkadot` (individuality-apps#1281); in this first stage both are placed, so the game's worker
 * keeps running until Humanity's worker has taken the room over and the game's worker has cancelled
 * the pushes only it can cancel. A later release stops placing the game.
 *
 * Placing one grants no permissions; it still prompts like any other product.
 */
@Singleton
class HostPlacedProducts @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    private class Definition(
        @StringRes val nameRes: Int,
        val backingProduct: (DotNsTld) -> ProductId,
        /**
         * Must match the room the product registers, which the host has no way to check. The
         * host pins this room and no other the product has.
         */
        val roomId: String,
    )

    private val definitions = listOf(
        Definition(RCommon.string.host_placed_chat_polkadot, ReservedProductIds::personhood, roomId = "polkadot"),
        Definition(RCommon.string.host_placed_chat_jollity, ReservedProductIds::game, roomId = "jollity"),
    )

    /**
     * Nothing until the TLD is known, so the chats a device already has are not held back by a
     * chain read; a TLD persisted by a previous process costs none.
     */
    fun observe(): Flow<List<HostPlacedProduct>> = flow {
        val tld = dotNsTldProvider.currentTldOrNull() ?: run {
            emit(emptyList())
            dotNsTldProvider.getTldRetrying()
        }
        emit(placements(tld))
    }

    private fun placements(tld: DotNsTld) = definitions.map {
        HostPlacedProduct(
            product = Product(it.backingProduct(tld), context.getString(it.nameRes), icon = null),
            room = ProductChatIdParameter(it.roomId),
        )
    }
}

/** A product the host places in chat, and the one room of it the host places. */
data class HostPlacedProduct(val product: Product, val room: ProductChatIdParameter)
