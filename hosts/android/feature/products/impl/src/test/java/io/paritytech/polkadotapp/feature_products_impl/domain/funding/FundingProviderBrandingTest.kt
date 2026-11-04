package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.InMemoryBundledProductFiles
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class FundingProviderBrandingTest {
    private val provider = ProductId.parse("coinflow.dot").getOrThrow()
    private val bundledProvider = ProductId.parse("bundled.dot").getOrThrow()
    private val resolveProduct = mockk<ResolveProductUseCase>()
    private val bundledProducts = BundledProducts(
        InMemoryBundledProductFiles(mapOf("bundled-products/bundled.dot/worker/index.js" to "export {}")),
        enabled = true,
    )
    private val branding = FundingProviderBranding(resolveProduct, mockk(), bundledProducts)

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
    fun `a provider the app ships is shown by its id with no manifest read`() = runTest {
        assertEquals(FundingProviderBrand(bundledProvider, "bundled.dot", iconUrl = null), branding.brand(bundledProvider))
        coVerify(exactly = 0) { resolveProduct.resolve(any()) }
    }
}
