package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.bundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class FundingProviderBrandingTest {
    private val provider = ProductId.parse("coinflow.dot").getOrThrow()
    private val bundledProvider = ProductId.parse("bundled.dot").getOrThrow()
    private val resolveProduct = mockk<ResolveProductUseCase>()
    private val bundledProducts = bundledProducts(
        mapOf(
            "bundled-products/bundled.dot/manifest.json" to """{"${'$'}v":1,"displayName":"Bundled","icon":{"cid":"bafyicon"}}""",
            "bundled-products/bundled.dot/worker/index.js" to "export {}",
            "bundled-products/unnamed.dot/worker/index.js" to "export {}",
        ),
    )
    private val ipfs = mockk<IpfsContentLookup> { coEvery { getIpfsLinkFor("bafyicon") } returns Result.success(ICON_URL) }
    private val branding = FundingProviderBranding(resolveProduct, ipfs, bundledProducts)

    @Test
    fun `a provider is shown by its product's name`() = runTest {
        coEvery { resolveProduct.resolve(provider) } returns Result.success(
            mockk<ResolvedProduct> {
                every { product.name } returns "Coinflow"
                every { product.icon } returns null
            },
        )

        assertEquals(FundingProviderBrand(provider, "Coinflow", iconUrl = null), branding.brand(provider))
    }

    @Test
    fun `the bare id stands in while the product does not resolve`() = runTest {
        coEvery { resolveProduct.resolve(provider) } returns Result.failure(IllegalStateException("offline"))

        assertEquals(FundingProviderBrand(provider, "coinflow.dot", iconUrl = null), branding.brand(provider))
    }

    @Test
    fun `a provider the app ships is shown by the root manifest it ships, with no lookup`() = runTest {
        val unnamed = ProductId.parse("unnamed.dot").getOrThrow()

        assertEquals(FundingProviderBrand(bundledProvider, "Bundled", ICON_URL), branding.brand(bundledProvider))
        assertEquals(FundingProviderBrand(unnamed, "unnamed.dot", iconUrl = null), branding.brand(unnamed))
        coVerify(exactly = 0) { resolveProduct.resolve(any()) }
    }

    private companion object {
        const val ICON_URL = "https://gateway/ipfs/bafyicon"
    }
}
