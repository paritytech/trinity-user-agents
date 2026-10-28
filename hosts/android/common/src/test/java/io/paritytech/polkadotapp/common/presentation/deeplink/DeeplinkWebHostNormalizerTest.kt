package io.paritytech.polkadotapp.common.presentation.deeplink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.productShareHosts
import java.io.File

class DeeplinkWebHostNormalizerTest {
    @Test
    fun `every supported share domain reaches the app through its intent filters`() {
        val manifest = File("../app/src/main/AndroidManifest.xml").readText()
        val hosts = productShareHosts().flatMap { listOf(it, "*.$it") }

        hosts.forEach { host -> assertTrue(host, manifest.contains("android:host=\"$host\"")) }
    }

    @Test
    fun `bare app links take the first path segment as their action`() {
        listOf("http", "https").forEach { scheme ->
            assertEquals("pay", bareHostDeeplinkAction(scheme, "dot.li", listOf("pay", "cheque")))
        }
    }

    @Test
    fun `bare app links need an action path`() {
        assertNull(bareHostDeeplinkAction("https", "dot.li", emptyList()))
    }

    @Test
    fun `only an app link apex host interprets its path as an action`() {
        listOf("pair.dot.li", "example.com").forEach { host ->
            assertNull(bareHostDeeplinkAction("https", host, listOf("pair")))
        }
    }

    @Test
    fun `bare app links require a web scheme and host`() {
        listOf("polkadotapp", "file", null).forEach { scheme ->
            assertNull(bareHostDeeplinkAction(scheme, "dot.li", listOf("pair")))
        }
        assertNull(bareHostDeeplinkAction("https", null, listOf("pair")))
    }
}
