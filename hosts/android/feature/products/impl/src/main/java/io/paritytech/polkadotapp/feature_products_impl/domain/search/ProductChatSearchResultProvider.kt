package io.paritytech.polkadotapp.feature_products_impl.domain.search

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.search.ChatListSearchResult
import io.paritytech.polkadotapp.feature_chats_api.domain.search.ChatSearchResultProvider
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.first

class ProductChatSearchResultProvider(
    private val arbitraryProductsEnabled: Boolean,
    private val productRepository: ProductRepository,
    private val productsRouter: ProductsRouter,
) : ChatSearchResultProvider {
    override val id: ChatExtensionId = PRODUCT_SEARCH_PROVIDER_ID

    override suspend fun search(query: String): Result<List<ChatListSearchResult.App>> {
        if (!arbitraryProductsEnabled) return Result.success(emptyList())

        return runCatching {
            productRepository.observeProducts()
                .first()
                .filter { it.name.contains(query, ignoreCase = true) }
                .map { product ->
                    ChatListSearchResult.App(
                        id = product.id.value,
                        title = product.name,
                        providerId = id,
                    )
                }
        }
    }

    override suspend fun onAppResultSelected(result: ChatListSearchResult.App) {
        productsRouter.openSpaBrowser(SpaBrowserPayload.ByProductId(result.id))
    }
}

// Shares the namespace root of ProductId.toChatExtensionId(), which mints "ProductBot_<productId>".
private const val PRODUCT_SEARCH_PROVIDER_ID: ChatExtensionId = "ProductBot"
