package io.paritytech.polkadotapp.feature_products_impl.data.bundled

import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.bundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.productSources
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.ProductId

class ProductContentsTest {
    private val bundled = bundledProducts(
        mapOf(
            "bundled-products/worker.paseo/worker-manifest.json" to
                """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":true}}""",
            "bundled-products/worker.paseo/worker/index.js" to "export {}",
            "bundled-products/worker.paseo/app/index.html" to "<html></html>",
        ),
    )
    private val sources = productSources(bundled)
    private val contents = ProductContents(bundled, sources)

    // The WebView of a worker the app ships serves its script from the bundle, under the worker's own origin.
    @Test
    fun `a bundled worker is served from the bundle`() {
        assertEquals("worker.worker.paseo", contents.worker(ProductId.parse("worker.paseo").getOrThrow())?.host)
    }

    @Test
    fun `a bundled product's pages are served from the bundle under its id`() {
        assertEquals("worker.paseo", contents.app(sources.of("worker.paseo"), emptyMap())?.host)
    }

    // Anything else is the dotNS or network clients' to load.
    @Test
    fun `a published product is served by nothing here`() {
        val published = ProductId.parse("other.paseo").getOrThrow()

        assertNull(contents.worker(published))
        assertNull(contents.app(sources.of("other.paseo"), emptyMap()))
        assertNull(contents.app(null, emptyMap()))
    }
}
