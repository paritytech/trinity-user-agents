package io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser

import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
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

interface SpaBrowserInteractor {
    suspend fun productIdOrNull(url: String): ProductId?

    suspend fun resolveProduct(productId: ProductId): Result<ResolvedProduct>

    suspend fun installChatAndAwaitRoomCreated(productId: ProductId): Result<ChatId>

    fun share(productId: ProductId)
}

class RealSpaBrowserInteractor @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
    private val integrationRepository: ProductIntegrationRepository,
    private val chatRoomRepository: ReadOnlyChatRoomRepository,
    private val sharingManager: SharingManager,
    private val dotNsTldProvider: DotNsTldProvider,
) : SpaBrowserInteractor {
    // Waits rather than guesses: a tab classified before the TLD lands would stick as "not a product"
    // for its whole lifetime.
    override suspend fun productIdOrNull(url: String): ProductId? =
        productIdFromUrl(url.toUri(), dotNsTldProvider.getTldRetrying()).getOrNull()

    override suspend fun resolveProduct(productId: ProductId): Result<ResolvedProduct> =
        resolveProductUseCase.resolve(productId)

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
