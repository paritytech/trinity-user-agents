@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.toCanonicalDotHost
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.chatRoomIdOf
import kotlinx.coroutines.flow.first
import java.net.URI
import java.net.URISyntaxException
import javax.inject.Inject
import kotlin.time.ExperimentalTime

/**
 * Tells whether a notification of a product opens a room of the same product that the user muted (D34). The core
 * silences such a notification when the product sends it for now. The host also checks each notification that it
 * shows, because the core does not see a scheduled notification when its alarm fires (07-LATER §6.2).
 */
class MutedProductRoomCheck @Inject constructor(
    private val userStateRepository: ChatRoomUserStateRepository,
    private val timeProvider: TimeProvider,
) {
    suspend fun isMuted(productId: ProductId, deeplink: String?): Boolean {
        val roomId = deeplink?.let { productRoomOfLink(productId, it) } ?: return false
        val chatId = ProductChatIdParameter(roomId).toChatId(productId.toChatExtensionId())

        return userStateRepository.observe(chatId).first().isMutedAt(timeProvider.now().toEpochMilliseconds())
    }
}

/**
 * The room of a link `polkadot://<product>/-/chat/<room>` (02-DESIGN §13 item 15) when `<product>` is [productId],
 * with its escapes decoded. Null for a link to another product or to another page. A link of a product never
 * names a room of another product, as in the core (`chat_room_of_link`).
 */
internal fun productRoomOfLink(productId: ProductId, link: String): String? {
    val uri = try {
        URI(link)
    } catch (_: URISyntaxException) {
        return null
    }
    val host = uri.host?.lowercase()?.toCanonicalDotHost() ?: return null
    if (host != productId.value) return null

    return chatRoomIdOf(uri.path.orEmpty().split('/').filter { it.isNotEmpty() })
}
