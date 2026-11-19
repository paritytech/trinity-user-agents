package io.parity.truapi

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class ProductIdTest {
    @Test
    fun equalNativeValuesShareCollectionKeys() {
        val first = ProductId.parse("peopl.dot").getOrThrow()
        val second = ProductId.parse("peopl.dot").getOrThrow()

        assertEquals(mapOf(second to "account"), mapOf(first to "account"))
    }
}
