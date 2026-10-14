package io.paritytech.polkadotapp.feature_dotns_api.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DotNsTldTest {
    @Test
    fun `parses a valid dotted suffix`() {
        assertEquals("paseo", DotNsTld.parse("paseo")?.value)
        assertEquals("dot", DotNsTld.parse("dot")?.value)
        assertEquals("test", DotNsTld.parse("test")?.value)
    }

    @Test
    fun `exposes the dotted suffix`() {
        assertEquals(".paseo", DotNsTld.parse("paseo")?.suffix)
    }

    @Test
    fun `rejects a dotted suffix`() {
        assertNull(DotNsTld.parse(".paseo"))
    }

    @Test
    fun `rejects a multi-label suffix`() {
        assertNull(DotNsTld.parse("paseo.li"))
    }

    @Test
    fun `rejects an empty or malformed suffix`() {
        assertNull(DotNsTld.parse(""))
        assertNull(DotNsTld.parse("Paseo"))
        assertNull(DotNsTld.parse("pa seo"))
        assertNull(DotNsTld.parse("-paseo"))
    }
}
