package io.paritytech.polkadotapp.feature_products_api.presentation.deeplink

import io.paritytech.polkadotapp.feature_products_api.domain.FundingConfig
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductDeepLinkGateTest {
    private val onramp = ProductId.fromStoredValue("fund.dot")
    private val arbitrary = ProductId.fromStoredValue("anything.dot")

    private class FundingProducts(private val ids: Set<ProductId>) : FundingDomainProvider {
        override suspend fun getFundingConfig(): Result<FundingConfig> =
            Result.failure(UnsupportedOperationException("unused"))

        override suspend fun getFundingProductIds(): Result<Set<ProductId>> = Result.success(ids)
    }

    private fun gate(arbitraryProductsEnabled: Boolean, funding: Set<ProductId> = setOf(onramp)) =
        ProductDeepLinkGate(arbitraryProductsEnabled, FundingProducts(funding))

    // The flag is what keeps an unvetted product off the screen: a link that resolves its manifest
    // also runs its worker and hosts its pages, which is exactly what a safety build refuses.
    @Test
    fun `off the flag, only the app's own funding products are reachable`() = runBlocking {
        assertTrue(gate(arbitraryProductsEnabled = false).opens(onramp))
        assertFalse(gate(arbitraryProductsEnabled = false).opens(arbitrary))
    }

    @Test
    fun `on the flag, any product is reachable`() = runBlocking {
        assertTrue(gate(arbitraryProductsEnabled = true).opens(arbitrary))
    }

    // A link whose host is not a product at all reads as arbitrary, never as built in.
    @Test
    fun `a link that names no product is refused off the flag and allowed on it`() = runBlocking {
        assertFalse(gate(arbitraryProductsEnabled = false).opens(null))
        assertTrue(gate(arbitraryProductsEnabled = true).opens(null))
    }

    @Test
    fun `an unreadable funding configuration refuses rather than opens`() = runBlocking {
        val unreadable = object : FundingDomainProvider {
            override suspend fun getFundingConfig(): Result<FundingConfig> =
                Result.failure(IllegalStateException("offline"))

            override suspend fun getFundingProductIds(): Result<Set<ProductId>> =
                Result.failure(IllegalStateException("offline"))
        }

        assertFalse(ProductDeepLinkGate(arbitraryProductsEnabled = false, unreadable).opens(onramp))
    }
}
