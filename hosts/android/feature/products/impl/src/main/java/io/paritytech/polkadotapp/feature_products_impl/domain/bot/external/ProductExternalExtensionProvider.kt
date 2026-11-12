package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotFactory
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
            val placedExtensions = placed.map { productBotFactory.create(it, ProductChatExtensionOrigin.PlacedByHost) }
            val installedExtensions = installed
                .filterNot { it.id in placedIds }
                .map { productBotFactory.create(it, ProductChatExtensionOrigin.InstalledByUser) }

            placedExtensions + installedExtensions
        }.distinctUntilChanged()
    }
}
