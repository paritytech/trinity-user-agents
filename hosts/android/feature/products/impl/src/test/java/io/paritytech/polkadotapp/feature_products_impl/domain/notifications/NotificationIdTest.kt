package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class NotificationIdTest {
    /**
     * Two notifications sharing an id would replace each other in the shade while each still
     * reported itself as shown, so a product asking twice in the same millisecond gets two ids.
     */
    @Test
    fun `ids a product is given in one millisecond are distinct`() {
        val productId = ProductId.parse("jolly.dot").getOrThrow()

        val ids = (1..1000).map { NotificationId.generate(productId).value }

        assertEquals(ids.size, ids.toSet().size)
    }
}
