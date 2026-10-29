package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FundingReturnUrlTest {
    private val returnUrl = "https://provider.example/return"

    @Test
    fun `the return page with the onramp's result in its query is reached`() {
        assertTrue(reachesReturnUrl("https://provider.example/return?status=success&id=42", returnUrl))
        assertTrue(reachesReturnUrl("https://PROVIDER.example:443/return#done", returnUrl))
    }

    @Test
    fun `another page, host or scheme is not`() {
        assertFalse(reachesReturnUrl("https://provider.example/return/more", returnUrl))
        assertFalse(reachesReturnUrl("https://evil.example/return", returnUrl))
        assertFalse(reachesReturnUrl("http://provider.example/return", returnUrl))
        assertFalse(reachesReturnUrl("not a url", returnUrl))
    }
}
