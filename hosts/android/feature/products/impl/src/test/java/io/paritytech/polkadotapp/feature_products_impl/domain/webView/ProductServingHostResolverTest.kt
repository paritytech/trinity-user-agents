package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId

class ProductServingHostResolverTest {
    private val resolveProductUseCase: ResolveProductUseCase = mock()
    private val dotNsTldProvider: DotNsTldProvider = mock<DotNsTldProvider>().also {
        runBlocking { whenever(it.getTld()).thenReturn(Result.success(DotNsTld.parse("dot")!!)) }
    }
    private val resolver = ProductServingHostResolver(resolveProductUseCase, dotNsTldProvider)

    private fun resolvedWith(appHost: String?, widgetHost: String?) = ResolvedProduct(
        product = Product(id = ProductId.parse("coinflip.dot").getOrThrow(), name = "Coinflip", icon = null),
        description = null,
        executables = Executables(
            app = appHost?.let { ProductExecutable.App(ExecutableHost(it), SemVer.ZERO) },
            widget = widgetHost?.let {
                ProductExecutable.Widget(ExecutableHost(it), SemVer.ZERO, description = null, heights = listOf(1), width = 1)
            },
            worker = null,
        ),
    )

    private suspend fun stubResolution(appHost: String?, widgetHost: String? = null) {
        whenever(resolveProductUseCase.resolve(ProductId.parse("coinflip.dot").getOrThrow()))
            .thenReturn(Result.success(resolvedWith(appHost, widgetHost)))
    }

    @Test
    fun `a manifest product is served from its app subname`() = runBlocking {
        stubResolution("app.coinflip.dot")

        assertEquals("app.coinflip.dot", resolver.servingHostFor("coinflip.dot").getOrThrow())
    }

    @Test
    fun `a legacy product is served from its own host`() = runBlocking {
        stubResolution("coinflip.dot")

        assertEquals("coinflip.dot", resolver.servingHostFor("coinflip.dot").getOrThrow())
    }

    @Test
    fun `a product with no app surface fails`() = runBlocking {
        stubResolution(null)

        assertTrue(resolver.servingHostFor("coinflip.dot").isFailure)
        assertTrue(resolver.servingHostFor("coinflip.dot", ExecutableKind.WIDGET).isFailure)
    }

    // An expanded card runs the product's widget when it publishes one, not its app.
    @Test
    fun `a card's page is served from the widget subname`() = runBlocking {
        stubResolution(appHost = "app.coinflip.dot", widgetHost = "widget.coinflip.dot")

        assertEquals("widget.coinflip.dot", resolver.servingHostFor("coinflip.dot", ExecutableKind.WIDGET).getOrThrow())
    }

    // A product can publish cards without a widget, and its card still has to open.
    @Test
    fun `a card of a product with no widget is served its app`() = runBlocking {
        stubResolution(appHost = "app.coinflip.dot")

        assertEquals("app.coinflip.dot", resolver.servingHostFor("coinflip.dot", ExecutableKind.WIDGET).getOrThrow())
    }

    @Test
    fun `a web-mirror host maps to the canonical archive`() = runBlocking {
        stubResolution("app.coinflip.dot")

        assertEquals("app.coinflip.dot", resolver.servingHostFor("coinflip.dot.li").getOrThrow())
    }
}
