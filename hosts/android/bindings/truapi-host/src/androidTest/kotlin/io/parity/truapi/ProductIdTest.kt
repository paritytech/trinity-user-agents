package io.parity.truapi

import android.os.Parcel
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class ProductIdTest {
    @Test
    fun equalNativeValuesShareCollectionKeys() {
        val first = ProductId.parse("peopl.dot")
        val second = ProductId.parse("peopl.dot")

        assertEquals(mapOf(second to "account"), mapOf(first to "account"))
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
