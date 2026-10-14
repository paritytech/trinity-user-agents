package io.paritytech.polkadotapp.feature_products_impl.domain.search

import io.paritytech.polkadotapp.feature_chats_api.domain.model.search.ChatListSearchResult
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify

class ProductChatSearchResultProviderTest {
    private val productRepository: ProductRepository = mock()
    private val productsRouter: ProductsRouter = mock()
    private val provider = provider(arbitraryProductsEnabled = true)

    @Before
    fun setUp() {
        val products = listOf(
            Product(
                id = ProductId.fromStoredValue("coinflip.dot"),
                name = "Coinflip",
                icon = null,
            ),
            Product(
                id = ProductId.fromStoredValue("gamble.dot"),
                name = "Gamble Game",
                icon = null,
            ),
            Product(
                id = ProductId.fromStoredValue("nft.dot"),
                name = "NFT Gallery",
                icon = null,
            ),
        )
        runBlocking {
            whenever(productRepository.observeProducts()).thenReturn(flowOf(products))
        }
    }

    @Test
    fun `returns matching products when query matches name substring`() = runBlocking {
        val result = provider.search("game")

        assertTrue(result.isSuccess)
        val apps = result.getOrThrow()
        assertEquals(1, apps.size)
        assertEquals("gamble.dot", apps[0].id)
        assertEquals("Gamble Game", apps[0].title)
    }

    @Test
    fun `filters case-insensitively`() = runBlocking {
        val result = provider.search("COIN")

        assertTrue(result.isSuccess)
        val apps = result.getOrThrow()
        assertEquals(1, apps.size)
        assertEquals("coinflip.dot", apps[0].id)
    }

    @Test
    fun `returns multiple matching products`() = runBlocking {
        val result = provider.search("a")

        assertTrue(result.isSuccess)
        val apps = result.getOrThrow()
        assertEquals(2, apps.size)
    }

    @Test
    fun `returns empty list when query does not match any product`() = runBlocking {
        val result = provider.search("nonexistent")

        assertTrue(result.isSuccess)
        val apps = result.getOrThrow()
        assertEquals(0, apps.size)
    }

    @Test
    fun `returns all products for empty query`() = runBlocking {
        val result = provider.search("")

        assertTrue(result.isSuccess)
        val apps = result.getOrThrow()
        assertEquals(3, apps.size)
    }

    @Test
    fun `stamps its own provider id onto every result`() = runBlocking {
        val apps = provider.search("").getOrThrow()

        assertTrue(apps.all { it.providerId == provider.id })
    }

    @Test
    fun `opens spa browser for the selected result`() = runBlocking<Unit> {
        provider.onAppResultSelected(appResult("coinflip.dot"))

        verify(productsRouter).openSpaBrowser(SpaBrowserPayload.ByProductId("coinflip.dot"))
    }

    @Test
    fun `returns nothing when arbitrary products are disabled`() = runBlocking {
        val result = provider(arbitraryProductsEnabled = false).search("")

        assertTrue(result.isSuccess)
        assertEquals(0, result.getOrThrow().size)
    }

    private fun provider(arbitraryProductsEnabled: Boolean) = ProductChatSearchResultProvider(
        arbitraryProductsEnabled = arbitraryProductsEnabled,
        productRepository = productRepository,
        productsRouter = productsRouter,
    )

    private fun appResult(id: String) = ChatListSearchResult.App(
        id = id,
        title = "Test Product",
        providerId = provider.id,
    )
}
