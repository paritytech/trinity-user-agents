package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ProductExternalExtensionProvider @Inject constructor(
    private val integrationRepository: ProductIntegrationRepository,
    private val productBotFactory: ProductBotFactory,
) : ExternalExtensionProvider {
    override fun observeExtensions(): Flow<List<ExternalExtension>> {
        return integrationRepository.observeProductsByType(IntegrationType.Chat)
            .distinctUntilChangedBy { it.toSet() }
            .map { products ->
                products.map { product -> productBotFactory.create(product) }
            }
    }
}
