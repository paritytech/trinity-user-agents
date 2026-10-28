package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.NativeRendererException

class PocketFaceJsonTest {
    @Test
    fun `a refused preview is returned as a failure instead of escaping the reader`() {
        assertTrue(readPocketFace("not json").exceptionOrNull() is NativeRendererException.Malformed)
    }
}
