package io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser

import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import javax.inject.Inject

interface SpaBrowserInteractor {
    suspend fun installChatAndAwaitRoomCreated(productId: ProductId): Result<ChatId>

    suspend fun resolveProduct(productId: ProductId): Result<ResolvedProduct>
}

class RealSpaBrowserInteractor @Inject constructor(
    private val integrationRepository: ProductIntegrationRepository,
    private val chatRoomRepository: ReadOnlyChatRoomRepository,
    private val resolveProductUseCase: ResolveProductUseCase,
) : SpaBrowserInteractor {
    override suspend fun resolveProduct(productId: ProductId): Result<ResolvedProduct> =
        resolveProductUseCase.resolve(productId)

    override suspend fun installChatAndAwaitRoomCreated(productId: ProductId): Result<ChatId> = runCatching {
        integrationRepository.install(productId, IntegrationType.Chat)

        val extensionId = productId.toChatExtensionId()

        chatRoomRepository.subscribeRoomsByExtension(extensionId)
            .mapNotNull { rooms -> rooms.firstOrNull() }
            .first()
    }
}
