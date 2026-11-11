package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.bundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.productSources
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId

class RealProductScriptResolverTest {
    private val resolveProduct = mockk<ResolveProductUseCase>()
    private val bundled = bundledProducts(
        mapOf(
            "bundled-products/worker.paseo/worker-manifest.json" to
                """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":true}}""",
            "bundled-products/worker.paseo/worker/index.js" to "export {}",
            "bundled-products/broken.paseo/worker-manifest.json" to """{"${'$'}v":1,"kind":"worker"}""",
            "bundled-products/broken.paseo/worker/index.js" to "export {}",
        ),
    )
    private val resolver = RealProductScriptResolver(resolveProduct, productSources(bundled), bundled)

    @Test
    fun `a bundled Worker is read from its files without asking dotNS`() = runTest {
        val worker = resolver.resolveWorker(ProductId.parse("worker.paseo").getOrThrow()).getOrThrow()

        assertEquals("https://worker.worker.paseo/index.js", worker.scriptUrl)
        assertTrue(worker.includesChat)
        coVerify(exactly = 0) { resolveProduct.resolve(any()) }
    }

    @Test
    fun `a bundled Worker manifest the parser rejects fails the resolve`() = runTest {
        assertTrue(resolver.resolveWorker(ProductId.parse("broken.paseo").getOrThrow()).isFailure)
    }
}
