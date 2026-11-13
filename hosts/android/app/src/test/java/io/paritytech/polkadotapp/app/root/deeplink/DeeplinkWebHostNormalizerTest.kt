package io.paritytech.polkadotapp.app.root.deeplink

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.appDeeplinkScheme
import uniffi.truapi.productShareHosts
import java.io.File

// The TLDs of the stacks the app bundles: previewnet, paseo-next-v2 and Polkadot.
private val BUNDLED_TLDS = listOf("testnet", "paseo", "dot").map { DotNsTld.parse(it)!! }
private val DOT = DotNsTld.parse("dot")!!
private val PASEO = DotNsTld.parse("paseo")!!

class DeeplinkWebHostNormalizerTest {
    @Test
    fun `every network's share domains and the app scheme reach the app through its intent filters`() {
        val manifest = File("src/main/AndroidManifest.xml").readText()
        val hosts = BUNDLED_TLDS.flatMap { productShareHosts(it.value) }.flatMap { listOf(it, "*.$it") }
        val schemes = Regex("""android:scheme="([^"]+)"""").findAll(manifest).map { it.groupValues[1] }.toSet()

        hosts.forEach { host -> assertTrue(host, manifest.contains("android:host=\"$host\"")) }
        assertEquals(setOf("https", appDeeplinkScheme()), schemes)
    }

    @Test
    fun `bare app links take the first path segment as their action`() {
        listOf("http", "https").forEach { scheme ->
            assertEquals("pay", bareHostDeeplinkAction(scheme, "dot.li", listOf("pay", "cheque"), DOT))
        }
    }

    @Test
    fun `bare app links need an action path`() {
        assertNull(bareHostDeeplinkAction("https", "dot.li", emptyList(), DOT))
    }

    @Test
    fun `only an app link apex host interprets its path as an action`() {
        listOf("pair.dot.li", "example.com").forEach { host ->
            assertNull(bareHostDeeplinkAction("https", host, listOf("pair"), DOT))
        }
    }

    @Test
    fun `bare app links require a web scheme and host`() {
        listOf("polkadotapp", "file", null).forEach { scheme ->
            assertNull(bareHostDeeplinkAction(scheme, "dot.li", listOf("pair"), DOT))
        }
        assertNull(bareHostDeeplinkAction("https", null, listOf("pair"), DOT))
    }

    @Test
    fun `a network reads its own and Polkadot's share domains and no other`() {
        assertEquals("pair", bareHostDeeplinkAction("https", "paseo.li", listOf("pair"), PASEO))
        assertEquals("pair", bareHostDeeplinkAction("https", "dot.li", listOf("pair"), PASEO))
        assertNull(bareHostDeeplinkAction("https", "testnet.li", listOf("pair"), PASEO))
        assertEquals("browse.paseo", dotProductHost("https", "browse.paseo.li", PASEO))
        assertNull(dotProductHost("https", "browse.testnet.li", PASEO))
    }
}
