package io.paritytech.polkadotapp.feature_products_impl.domain.bot.model

import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductChatIconTest {
    @Test
    fun `an absent icon remains absent in the native model`() {
        assertNull(ProductChatIcon.fromString(null).getOrThrow())
    }

    @Test
    fun `a refused icon becomes a native validation failure`() {
        assertTrue(ProductChatIcon.fromString("javascript:alert(1)").exceptionOrNull() is IllegalArgumentException)
    }
}
