package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class ProductNotificationTitleTest {
    private val productId = ProductId.parse("acme.dot").getOrThrow()

    /** A trusted product's title replaces its id as the notification heading. */
    @Test
    fun `a named title heads the notification`() {
        assertEquals("Order shipped", productNotificationTitle("Order shipped", productId))
    }

    /** Without a title the product id heads the notification, so the person still sees who sent it. */
    @Test
    fun `the product id heads a notification without a title`() {
        assertEquals("acme.dot", productNotificationTitle(null, productId))
    }
}
