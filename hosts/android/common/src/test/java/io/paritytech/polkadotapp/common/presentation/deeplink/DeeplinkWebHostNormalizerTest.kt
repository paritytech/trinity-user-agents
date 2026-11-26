package io.paritytech.polkadotapp.common.presentation.deeplink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.BundledStack
import uniffi.truapi.ReservedValuesPolicy
import uniffi.truapi.Stack
import uniffi.truapi.appDeeplinkScheme
import java.io.File

// A stack from the assets the app ships, as the core opens it.
private fun bundledStack(name: String, allowsReservedValues: Boolean): Stack {
    val stacks = File(System.getProperty("truapi.stackAssets"), "stacks")
    return Stack.openBundled(
        BundledStack(
            pointer = File(stacks, "$name.link").readText().trim(),
            descriptor = File(stacks, "$name.json").readBytes(),
            reservedValues = ReservedValuesPolicy(allowed = allowsReservedValues),
        ),
    )
}

private val PASEO = bundledStack("paseo-next-v2", allowsReservedValues = false)

// The stacks the app bundles: previewnet, paseo-next-v2 and Polkadot.
private val BUNDLED_STACKS = listOf(
    bundledStack("previewnet", allowsReservedValues = false),
    PASEO,
    bundledStack("polkadot", allowsReservedValues = true),
)

class DeeplinkWebHostNormalizerTest {
    @Test
    fun `every network's share domains and the app scheme reach the app through its intent filters`() {
        val manifest = File("../app/src/main/AndroidManifest.xml").readText()
        val hosts = BUNDLED_STACKS.flatMap { it.productShareHosts() }.flatMap { listOf(it, "*.$it") }
        val schemes = Regex("""android:scheme="([^"]+)"""").findAll(manifest).map { it.groupValues[1] }.toSet()

        hosts.forEach { host -> assertTrue(host, manifest.contains("android:host=\"$host\"")) }
        assertEquals(setOf("https", appDeeplinkScheme()), schemes)
    }

    @Test
    fun `bare app links take the first path segment as their action`() {
        listOf("http", "https").forEach { scheme ->
            assertEquals("pay", bareHostDeeplinkAction(scheme, "dot.li", listOf("pay", "cheque"), PASEO))
        }
    }

    @Test
    fun `bare app links need an action path`() {
        assertNull(bareHostDeeplinkAction("https", "dot.li", emptyList(), PASEO))
    }

    @Test
    fun `only an app link apex host interprets its path as an action`() {
        listOf("pair.dot.li", "example.com").forEach { host ->
            assertNull(bareHostDeeplinkAction("https", host, listOf("pair"), PASEO))
        }
    }

    @Test
    fun `bare app links require a web scheme and host`() {
        listOf("polkadotapp", "file", null).forEach { scheme ->
            assertNull(bareHostDeeplinkAction(scheme, "dot.li", listOf("pair"), PASEO))
        }
        assertNull(bareHostDeeplinkAction("https", null, listOf("pair"), PASEO))
    }

    @Test
    fun `a stack reads its own and Polkadot's share domains and no other`() {
        assertEquals("pair", bareHostDeeplinkAction("https", "paseo.li", listOf("pair"), PASEO))
        assertEquals("pair", bareHostDeeplinkAction("https", "dot.li", listOf("pair"), PASEO))
        assertNull(bareHostDeeplinkAction("https", "testnet.li", listOf("pair"), PASEO))
        assertEquals("browse.paseo", dotProductHost("https", "browse.paseo.li", PASEO))
        assertNull(dotProductHost("https", "browse.testnet.li", PASEO))
    }

    @Test
    fun `a product under Polkadot's share domain is the stack's own product`() {
        assertEquals("browse.paseo", dotProductHost("https", "browse.dot.li", PASEO))
    }
}
