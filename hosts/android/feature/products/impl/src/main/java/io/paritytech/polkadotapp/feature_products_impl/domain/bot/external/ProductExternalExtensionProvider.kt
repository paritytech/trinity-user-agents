package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatExtension
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatExtensionOrigin
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ProductExternalExtensionProvider @Inject constructor(
    private val integrationRepository: ProductIntegrationRepository,
    private val productBotFactory: ProductBotFactory,
    private val hostPlacedProducts: HostPlacedProducts,
) : ExternalExtensionProvider {
    // One instance per listed product: the registry starts the first instance it sees and the feed
    // draws through the latest one, so a product listed again must come back as the same object.
    private val extensions = mutableMapOf<ProductId, ProductChatExtension>()

    /**
     * Host-placed products first, then the installed ones. A placed product the user also
     * installed is listed once, as the placed one: unioned rather than written to the product
     * tables, as Pocket does with its pinned card, so nothing can uninstall it, and kept the same
     * so the registry never stops and restarts its worker over a resolved name.
     */
    override fun observeExtensions(): Flow<List<ExternalExtension>> {
        return combine(
            hostPlacedProducts.observe(),
            integrationRepository.observeProductsByType(IntegrationType.Chat),
        ) { placed, installed ->
            val placedIds = placed.map { it.id }.toSet()
            val listed = placed.map { it to ProductChatExtensionOrigin.PlacedByHost } +
                installed.filterNot { it.id in placedIds }.map { it to ProductChatExtensionOrigin.InstalledByUser }

            synchronized(extensions) {
                extensions.keys.retainAll(listed.mapTo(mutableSetOf()) { (product, _) -> product.id })
                listed.map { (product, origin) -> extensionFor(product, origin) }
            }
        }.distinctUntilChanged()
    }

    private fun extensionFor(product: Product, origin: ProductChatExtensionOrigin): ProductChatExtension {
        val kept = extensions[product.id]?.takeIf { it.product == product && it.origin == origin }
        return kept ?: productBotFactory.create(product, origin).also { extensions[product.id] = it }
    }
}
