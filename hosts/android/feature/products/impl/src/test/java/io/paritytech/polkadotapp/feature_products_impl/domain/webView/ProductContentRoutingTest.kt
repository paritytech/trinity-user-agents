package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.product.RealProductContentWarmUp
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId
import java.io.File

class ProductContentRoutingTest {
    private val productId = ProductId.parse("coinflip.dot").getOrThrow()
    private val products: ResolveProductUseCase = mockk()
    private val tldProvider: DotNsTldProvider = mockk {
        coEvery { getTld() } returns Result.success(DotNsTld.parse("dot")!!)
    }
    private val archiveUri: Uri = mockk { every { path } returns "/archive" }
    private val dotNs: DotNsResolver = mockk {
        coEvery { resolveToLocalUri(any()) } returns Result.success(archiveUri)
    }
    private val serving = ProductServingHostResolver(products, tldProvider)
    private val runtime: ProductRuntimeSettings = mockk {
        every { isTrUAPIRuntimeEnabled() } returns true
    }
    private val warmUp = RealProductContentWarmUp(dotNs, serving, runtime)

    @Test
    fun `normalized product URLs fetch the published executable archive`() {
        coEvery { products.resolve(productId) } returns Result.success(resolved(app = "app.coinflip.dot"))
        val url: Uri = mockk { every { host } returns "COINFLIP.DOT." }

        assertEquals(File("/archive"), DotNsWebViewClient(dotNs, tldProvider, serving).resolveHostFile(url))

        coVerify(exactly = 1) { dotNs.resolveToLocalUri("app.coinflip.dot") }
    }

    @Test
    fun `the identity resolver still fetches canonical archive names`() {
        val url: Uri = mockk { every { host } returns "COINFLIP.DOT.LI." }

        assertEquals(File("/archive"), DotNsWebViewClient(dotNs, tldProvider).resolveHostFile(url))

        coVerify(exactly = 1) { dotNs.resolveToLocalUri("coinflip.dot") }
    }

    @Test
    fun `failed resolution stops WebView serving without fetching a base archive`() {
        coEvery { products.resolve(productId) } returns Result.failure(IllegalStateException("manifest unavailable"))
        val url: Uri = mockk { every { host } returns "coinflip.dot" }

        assertNull(DotNsWebViewClient(dotNs, tldProvider, serving).resolveHostFile(url))

        coVerify(exactly = 0) { dotNs.resolveToLocalUri(any()) }
    }

    @Test
    fun `ordinary hosts pass through without consulting product manifests`() = runBlocking {
        assertEquals("example.com", serving.servingHostFor("example.com").getOrThrow())

        coVerify(exactly = 0) { products.resolve(any()) }
    }

    @Test
    fun `a failed product lookup never warms the base archive`() = runBlocking {
        val failure = IllegalStateException("manifest unavailable")
        coEvery { products.resolve(productId) } returns Result.failure(failure)

        assertEquals(failure, warmUp.warmUp(productId).exceptionOrNull())

        coVerify(exactly = 0) { dotNs.resolveToLocalUri(any()) }
    }

    @Test
    fun `a missing widget never warms the base archive`() = runBlocking {
        coEvery { products.resolve(productId) } returns Result.success(resolved(app = "app.coinflip.dot"))

        assertTrue(warmUp.warmUp(productId).isFailure)

        coVerify(exactly = 0) { dotNs.resolveToLocalUri(any()) }
    }

    private fun resolved(app: String?) = ResolvedProduct(
        product = Product(productId, "Coinflip", null),
        description = null,
        executables = Executables(
            app = app?.let { ProductExecutable.App(ExecutableHost(it), SemVer.ZERO) },
            widget = null,
            worker = null,
        ),
    )
}
