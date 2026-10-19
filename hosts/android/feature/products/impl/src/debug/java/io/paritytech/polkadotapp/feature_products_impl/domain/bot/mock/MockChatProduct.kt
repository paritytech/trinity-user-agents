package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MockChatProductSettings
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatExtensionOrigin
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

/**
 * A chat product that lives in the app, so a tester can check how the host draws everything that a product can declare
 * and send (Chat Extension v2 and its additions) without a worker or a network. Its chat extension is a product chat
 * extension like any other; only the worker differs ([MockChatRuntime]).
 */
object MockChatProduct {
    // A dotNS name, so that room links of the showcase open it. Nothing resolves it.
    val productId = ProductId.fromStoredValue("chat-showcase.dot")

    val product = Product(productId, "Chat Showcase", icon = null)
}

@Singleton
class MockChatExtensionProvider @Inject constructor(
    private val settings: MockChatProductSettings,
    private val productBotFactory: ProductBotFactory,
) : ExternalExtensionProvider {
    override fun observeExtensions(): Flow<List<ExternalExtension>> = settings.observeEnabled()
        .distinctUntilChanged()
        .map { enabled ->
            if (enabled) listOf(productBotFactory.create(MockChatProduct.product, ProductChatExtensionOrigin.PlacedByHost)) else emptyList()
        }
}
