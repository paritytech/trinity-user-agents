package io.paritytech.polkadotapp.feature_products_api.model

import org.junit.Assert.assertEquals
import org.junit.Test

class SemVerTest {
    @Test
    fun `renders a three-part version`() {
        assertEquals("1.2.3", SemVer.fromComponents(1, 2, 3, build = null).toString())
    }

    @Test
    fun `renders a build identifier`() {
        assertEquals("1.0.0+abc123", SemVer.fromComponents(1, 0, 0, build = "abc123").toString())
    }

    @Test
    fun `an empty build identifier is no build identifier`() {
        assertEquals("1.0.0", SemVer.fromComponents(1, 0, 0, build = "").toString())
    }

    @Test
    fun `zero is the legacy placeholder`() {
        assertEquals("0.0.0", SemVer.ZERO.toString())
    }
}
