package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import android.content.Context
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatExtension
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatExtensionOrigin
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.truapi.ProductId
import uniffi.truapi.ProductIdException

class ProductExternalExtensionProviderTest {
    private val tld = requireNotNull(DotNsTld.parse("paseo"))
    private val jollity = Product(ReservedProductIds.game(tld), "Jollity", icon = null)
    private val playground = Product(ProductId.parse("truapi-playground.paseo").getOrThrow(), "TrUAPI Playground", icon = null)

    private val integrationRepository: ProductIntegrationRepository = mockk()
    private val hostPlacedProducts: HostPlacedProducts = mockk()
    private val provider = ProductExternalExtensionProvider(
        integrationRepository,
        ProductBotFactory(mockk<Context>(relaxed = true), mockk(), mockk(), mockk()),
        hostPlacedProducts,
    )

    private suspend fun extensions(stored: List<Product>, placed: List<Product>): List<ProductChatExtension> {
        every { integrationRepository.observeProductsByType(IntegrationType.Chat) } returns flowOf(stored)
        every { hostPlacedProducts.observe() } returns flowOf(placed)

        return provider.observeExtensions().first().map { it as ProductChatExtension }
    }

    @Test
    fun `an installed chat product is listed as its extension`() = runTest {
        val extensions = extensions(stored = listOf(playground), placed = emptyList())

        assertEquals(listOf(playground to ProductChatExtensionOrigin.InstalledByUser), extensions.map { it.product to it.origin })
    }

    @Test
    fun `an unsupported configured network is not silently omitted from host chats`() {
        val context: Context = mockk(relaxed = true)
        val tldProvider: DotNsTldProvider = mockk {
            every { currentTldOrNull() } returns DotNsTld.parse("test")!!
        }
        val provider = ProductExternalExtensionProvider(
            integrationRepository,
            ProductBotFactory(context, mockk(), mockk(), mockk()),
            HostPlacedProducts(context, tldProvider),
        )
        every { integrationRepository.observeProductsByType(IntegrationType.Chat) } returns flowOf(listOf(playground))

        assertThrows(ProductIdException::class.java) {
            runBlocking { provider.observeExtensions().first() }
        }
    }

    // Unioned rather than written to the product tables, as Pocket does with its pinned card:
    // nothing installs it, so nothing can uninstall it either.
    @Test
    fun `a host-placed product is listed with no stored row, marked as placed`() = runTest {
        val extensions = extensions(stored = listOf(playground), placed = listOf(jollity))

        assertEquals(
            listOf(jollity to ProductChatExtensionOrigin.PlacedByHost, playground to ProductChatExtensionOrigin.InstalledByUser),
            extensions.map { it.product to it.origin },
        )
    }

    // The extension registry stops and restarts an extension whose product changed, and with it
    // the worker. The placed product must stay the one listed, so an install from the browser
    // changes nothing; the room's own name and icon come from the product's createRoom.
    @Test
    fun `a host-placed product the user also installed is listed once, as the placed one`() = runTest {
        val installed = Product(jollity.id, "DIM2", icon = null)

        val extensions = extensions(stored = listOf(installed), placed = listOf(jollity))

        assertEquals(listOf(jollity), extensions.map { it.product })
    }
}
