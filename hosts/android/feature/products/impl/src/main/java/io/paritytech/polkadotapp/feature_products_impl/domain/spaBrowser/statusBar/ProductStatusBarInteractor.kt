package io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.statusBar

import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromUrl
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toUrl
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import uniffi.truapi.ProductId
import uniffi.truapi.productIdShareHost
import javax.inject.Inject

interface ProductStatusBarInteractor {
    suspend fun productIdOrNull(url: String): ProductId?

    suspend fun includesChat(productId: ProductId): Result<Boolean>

    suspend fun installChatAndAwaitRoomCreated(productId: ProductId): Result<ChatId>

    fun share(productId: ProductId)
}

class RealProductStatusBarInteractor @Inject constructor(
    private val integrationRepository: ProductIntegrationRepository,
    private val chatRoomRepository: ReadOnlyChatRoomRepository,
    private val resolveProductUseCase: ResolveProductUseCase,
    private val sharingManager: SharingManager,
    private val dotNsTldProvider: DotNsTldProvider,
) : ProductStatusBarInteractor {
    override suspend fun productIdOrNull(url: String): ProductId? =
        productIdFromUrl(url.toUri(), dotNsTldProvider.getTldRetrying()).getOrNull()

    override suspend fun includesChat(productId: ProductId): Result<Boolean> =
        resolveProductUseCase.resolve(productId).map { it.executables.worker?.includesChat == true }

    override suspend fun installChatAndAwaitRoomCreated(productId: ProductId): Result<ChatId> = runCancellableCatching {
        integrationRepository.install(productId, IntegrationType.Chat)

        val extensionId = productId.toChatExtensionId()

        chatRoomRepository.subscribeRoomsByExtension(extensionId)
            .mapNotNull { rooms -> rooms.firstOrNull() }
            .first()
    }

    override fun share(productId: ProductId) {
        val link = productIdShareHost(productId)?.let(Urls::ensureHasProtocolOrHttps) ?: productId.toUrl()
        sharingManager.shareText(link)
    }
}
