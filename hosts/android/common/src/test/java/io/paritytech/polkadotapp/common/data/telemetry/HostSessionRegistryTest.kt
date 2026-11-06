package io.paritytech.polkadotapp.common.data.telemetry

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HostSessionRegistryTest {
    // Checked under two TLDs because the registry's iteration order follows the names' hashes.
    @Test
    fun `a subname joins the session of the most specific open product it belongs to`() {
        for (tld in listOf("testnet", "dot")) {
            val registry = HostSessionRegistry()
            registry.open("peopl.$tld", "outer-session")
            registry.open("game.peopl.$tld", "inner-session")

            assertEquals("inner-session", registry.sessionIdForProductOrSubname("app.game.peopl.$tld"))
            assertEquals("outer-session", registry.sessionIdForProductOrSubname("app.peopl.$tld"))
        }
    }

    @Test
    fun `a name that only shares a suffix without a label boundary joins no session`() {
        val registry = HostSessionRegistry()
        registry.open("peopl.testnet", "session")

        assertNull(registry.sessionIdForProductOrSubname("notpeopl.testnet"))
    }
}
