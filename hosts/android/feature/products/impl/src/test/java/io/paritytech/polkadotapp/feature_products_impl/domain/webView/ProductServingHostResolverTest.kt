package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock

class ProductServingHostResolverTest {
    private val resolveProductUseCase: ResolveProductUseCase = mock()
    private val dotNsTldProvider: DotNsTldProvider = mock<DotNsTldProvider>().also {
        runBlocking { whenever(it.getTld()).thenReturn(Result.success(DotNsTld.parse("dot")!!)) }
    }
    private val resolver = ProductServingHostResolver(resolveProductUseCase, dotNsTldProvider)

    private fun resolvedWith(appHost: String?, widgetHost: String?) = ResolvedProduct(
        product = Product(id = ProductId.fromStoredValue("coinflip.dot"), name = "Coinflip", icon = null),
        executables = Executables(
            app = appHost?.let { ProductExecutable.App(ExecutableHost(it), SemVer.ZERO) },
            widget = widgetHost?.let {
                ProductExecutable.Widget(ExecutableHost(it), SemVer.ZERO, description = null, heights = listOf(1), width = 1)
            },
            worker = null,
        ),
    )

    private suspend fun stubResolution(appHost: String?, widgetHost: String? = null) {
        whenever(resolveProductUseCase.resolve(ProductId.fromStoredValue("coinflip.dot")))
            .thenReturn(Result.success(resolvedWith(appHost, widgetHost)))
    }

    @Test
    fun `a manifest product is served from its app subname`() = runBlocking {
        stubResolution("app.coinflip.dot")

        assertEquals("app.coinflip.dot", resolver.servingHostFor("coinflip.dot"))
    }

    @Test
    fun `a legacy product is served from its own host`() = runBlocking {
        stubResolution("coinflip.dot")

        assertEquals("coinflip.dot", resolver.servingHostFor("coinflip.dot"))
    }

    @Test
    fun `a product with no app surface is left alone`() = runBlocking {
        stubResolution(null)

        assertEquals("coinflip.dot", resolver.servingHostFor("coinflip.dot"))
    }

    // An expanded card runs the product's widget, so the page under a card must not be the app.
    @Test
    fun `a card's page is served from the widget subname`() = runBlocking {
        stubResolution(appHost = "app.coinflip.dot", widgetHost = "widget.coinflip.dot")

        assertEquals("widget.coinflip.dot", resolver.servingHostFor("coinflip.dot", ExecutableKind.WIDGET))
    }

    // A product with cards but no widget has faces that do not open; serving its app there instead
    // would run app code as a Widget.
    @Test
    fun `a card of a product with no widget is not served the app`() = runBlocking {
        stubResolution(appHost = "app.coinflip.dot")

        assertEquals("coinflip.dot", resolver.servingHostFor("coinflip.dot", ExecutableKind.WIDGET))
    }

    @Test
    fun `a host that already names an executable keeps its own archive`() = runBlocking {
        // Otherwise a worker.<base> request would be answered with the app's archive.
        assertEquals("worker.coinflip.dot", resolver.servingHostFor("worker.coinflip.dot"))
        assertEquals("app.coinflip.dot", resolver.servingHostFor("app.coinflip.dot"))
    }

    @Test
    fun `a web-mirror host maps to the canonical archive`() = runBlocking {
        stubResolution("app.coinflip.dot")

        assertEquals("app.coinflip.dot", resolver.servingHostFor("coinflip.dot.li"))
    }

    @Test
    fun `a host that is not a product is left alone`() = runBlocking {
        assertEquals("example.com", resolver.servingHostFor("example.com"))
    }
}
