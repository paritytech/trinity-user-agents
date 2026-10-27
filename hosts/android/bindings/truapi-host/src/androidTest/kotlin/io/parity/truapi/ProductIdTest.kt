package io.parity.truapi

import android.os.Parcel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.truapi.ProductId
import uniffi.truapi.ProductIdException

class ProductIdTest {
    @Test
    fun normalizedIdsShareCollectionKeys() {
        val first = ProductId.parse("  PEOPL.DOT  ")
        val second = ProductId.parse("peopl.dot")

        assertEquals(mapOf(second to "account"), mapOf(first to "account"))
    }

    @Test
    fun invalidInputReturnsAParsingError() {
        listOf("", "example.com", "my_app.dot", "localhost:3000.evil.dot").forEach { input ->
            assertThrows(ProductIdException::class.java) { ProductId.parse(input) }
        }
    }

    @Test
    fun executableIdentityIsPreserved() {
        assertEquals("app.peopl.paseo", ProductId.parse("APP.PEOPL.PASEO").value)
    }

    @Test
    fun parcelRoundTripPreservesIdentity() {
        val product = ProductId.parse("localhost:3000")
        val parcel = Parcel.obtain()
        try {
            product.writeToParcel(parcel, 0)
            parcel.setDataPosition(0)
            assertEquals(product, ProductId.CREATOR.createFromParcel(parcel))
        } finally {
            parcel.recycle()
        }
    }
}
