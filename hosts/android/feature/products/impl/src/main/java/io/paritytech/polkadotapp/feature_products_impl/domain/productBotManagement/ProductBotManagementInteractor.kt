package io.paritytech.polkadotapp.feature_products_impl.domain.productBotManagement

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotStateController
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCard
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import io.paritytech.polkadotapp.feature_products_impl.domain.product.UninstallProductUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

interface ProductBotManagementInteractor {
    fun observeProducts(): Flow<List<Product>>

    suspend fun getProduct(productId: ProductId): Product?

    suspend fun getUserWorkerUrl(productId: ProductId): String?

    suspend fun getDebugCard(productId: ProductId): DebugPocketCard?

    suspend fun isWasmiWorkerProduct(productId: ProductId): Boolean

    suspend fun upsertProduct(
        productId: ProductId,
        workerUrl: String,
        name: String,
        card: DebugPocketCard?,
        runsOnWasmi: Boolean,
    ): Result<ProductId>

    suspend fun updateProduct(
        productId: ProductId,
        workerUrl: String,
        name: String,
        card: DebugPocketCard?,
        runsOnWasmi: Boolean,
    ): Result<Unit>

    suspend fun deleteProduct(productId: ProductId): Result<Unit>

    suspend fun installChatIntegration(productId: ProductId): Result<Unit>

    fun currentTld(): DotNsTld?
}

/** Debug menu: a user-entered URL becomes the product's worker location via `userWorkerUrl`. */
@Singleton
class RealProductBotManagementInteractor @Inject constructor(
    private val productRepository: ProductRepository,
    private val integrationRepository: ProductIntegrationRepository,
    private val botStateController: ChatBotStateController,
    private val resolveProductUseCase: ResolveProductUseCase,
    private val uninstallProductUseCase: UninstallProductUseCase,
    private val dotNsTldProvider: DotNsTldProvider,
    private val debugPocketCards: DebugPocketCards,
    private val runtimeSettings: ProductRuntimeSettings,
    private val dispatchers: CoroutineDispatchers,
) : ProductBotManagementInteractor {
    override fun observeProducts(): Flow<List<Product>> {
        return productRepository.observeProducts()
    }

    override suspend fun getProduct(productId: ProductId): Product? {
        return productRepository.getProductById(productId)
    }

    override suspend fun getUserWorkerUrl(productId: ProductId): String? {
        return productRepository.getUserWorkerUrl(productId)
    }

    // The first read loads the preferences file from disk, and the edit dialog asks on the main thread.
    override suspend fun getDebugCard(productId: ProductId): DebugPocketCard? = withContext(dispatchers.io) {
        debugPocketCards.get(productId)
    }

    override suspend fun isWasmiWorkerProduct(productId: ProductId): Boolean = withContext(dispatchers.io) {
        runtimeSettings.isWasmiWorkerProduct(productId)
    }

    override suspend fun upsertProduct(
        productId: ProductId,
        workerUrl: String,
        name: String,
        card: DebugPocketCard?,
        runsOnWasmi: Boolean,
    ): Result<ProductId> {
        return runCatching {
            productRepository.upsertManualProduct(productId, name, workerUrl)
            debugPocketCards.set(productId, card)
            runtimeSettings.setWasmiWorkerProduct(productId, runsOnWasmi)
            resolveProductUseCase.invalidate(productId) // force next resolve to read the new URL
            integrationRepository.install(productId, IntegrationType.Chat)
            botStateController.setActive(productId.toChatExtensionId())
            productId
        }
    }

    override suspend fun updateProduct(
        productId: ProductId,
        workerUrl: String,
        name: String,
        card: DebugPocketCard?,
        runsOnWasmi: Boolean,
    ): Result<Unit> {
        return runCatching {
            productRepository.upsertManualProduct(productId, name, workerUrl)
            debugPocketCards.set(productId, card)
            runtimeSettings.setWasmiWorkerProduct(productId, runsOnWasmi)
            // The card rides on the resolved worker, so a changed one is only seen after this.
            resolveProductUseCase.invalidate(productId)
        }
    }

    override suspend fun deleteProduct(productId: ProductId): Result<Unit> {
        return uninstallProductUseCase(productId)
    }

    override suspend fun installChatIntegration(productId: ProductId): Result<Unit> {
        return runCatching {
            integrationRepository.install(productId, IntegrationType.Chat)
        }
    }

    override fun currentTld(): DotNsTld? {
        return dotNsTldProvider.currentTldOrNull()
    }
}
