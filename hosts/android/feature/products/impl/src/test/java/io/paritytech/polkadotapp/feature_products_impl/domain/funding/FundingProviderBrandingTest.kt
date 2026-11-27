package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class FundingProviderBrandingTest {
    private val provider = ProductId.parse("coinflow.dot").getOrThrow()
    private val resolveProduct = mockk<ResolveProductUseCase>()
    private val ipfs = mockk<IpfsContentLookup>()
    private val branding = FundingProviderBranding(resolveProduct, ipfs)

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
}
