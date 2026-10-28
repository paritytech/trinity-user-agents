package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductIdException

class ProductNavigationTest {
    private val dot = DotNsTld.parse("dot")!!

    @Test
    fun `Rust construction errors are returned as failures`() {
        val result = productIdFromHost("", dot)

        assertTrue(result.exceptionOrNull() is ProductIdException)
    }
}
