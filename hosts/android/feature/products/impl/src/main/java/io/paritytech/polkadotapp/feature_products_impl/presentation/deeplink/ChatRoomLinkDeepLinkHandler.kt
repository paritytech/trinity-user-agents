package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.presentation.deeplink.asWebUri
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.feature_products_api.model.toUrl
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.ProductDeepLinkGate
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.chatRoomIdOf
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.isChatRoomTarget
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import javax.inject.Inject

/**
 * `polkadot://<product>.<tld>/-/chat/<room>`, a link into a room of a product (02-DESIGN §13 item 15). The core reads
 * the link as a dotNS page, so the host reads the path itself (as iOS). A room that the host does not know yet opens
 * the App of its product, because the product creates its rooms when its worker runs.
 */
internal class ChatRoomLinkDeepLinkHandler @Inject constructor(
    private val dispatchers: CoroutineDispatchers,
    private val dotNsTldProvider: DotNsTldProvider,
    private val gate: ProductDeepLinkGate,
    private val chatRoomRepository: ReadOnlyChatRoomRepository,
    private val router: ProductsRouter,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        val tld = dotNsTldProvider.currentTldOrNull() ?: return false
        if (!DotNsUtils.isDotDomain(data.asWebUri(), tld) || !data.isChatRoomTarget()) return false

        return gate.opens(ProductId.fromUrl(data.asWebUri(), tld).getOrNull())
    }

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(dispatchers.io) {
        dotNsTldProvider.getTld().mapCatching { tld ->
            val productId = ProductId.fromUrl(data.asWebUri(), tld).getOrThrow()
            val roomId = checkNotNull(chatRoomIdOf(data.pathSegments)) { "Not a chat room link: $data" }
            val extensionId = productId.toChatExtensionId()
            val chatId = ProductChatIdParameter(roomId).toChatId(extensionId)
            val known = chatId in chatRoomRepository.subscribeRoomsByExtension(extensionId).first()

            DeeplinkProcessingOutcome.Navigate {
                if (known) router.openChat(chatId) else router.openSpaBrowser(SpaBrowserPayload.ByUrl(productId.toUrl()))
            }
        }
    }
}
