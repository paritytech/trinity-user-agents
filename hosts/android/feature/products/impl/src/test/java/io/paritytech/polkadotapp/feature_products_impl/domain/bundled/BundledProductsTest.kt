package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import com.google.gson.Gson
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.domain.browser.DevServers
import io.paritytech.polkadotapp.feature_products_impl.domain.browser.ProductSources
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource
import uniffi.truapi.archiveFile
import java.io.ByteArrayInputStream
import java.io.InputStream

/** Asset files by path, listed the way the asset manager lists them: a directory's children, a file's none. */
class InMemoryBundledProductFiles(
    private val files: Map<String, String>,
    override val enabled: Boolean = true,
) : BundledProductFiles {
    override fun list(dir: String): List<String> {
        val prefix = "$dir/"
        return files.keys
            .filter { it.startsWith(prefix) }
            .map { it.removePrefix(prefix).substringBefore('/') }
            .distinct()
    }

    override fun exists(path: String): Boolean = path in files

    override fun readText(path: String): String? = files[path]

    override fun open(path: String): InputStream? = files[path]?.let { ByteArrayInputStream(it.toByteArray()) }
}

fun bundledProducts(files: Map<String, String>, enabled: Boolean = true) =
    BundledProducts(InMemoryBundledProductFiles(files, enabled), ManifestParser(Gson()))

fun productSources(products: BundledProducts, devServersEnabled: Boolean = true) = ProductSources(
    products,
    mockk<DevServers> { every { enabled } returns devServersEnabled },
    mockk { every { currentTldOrNull() } returns null },
)

private fun workerManifestJson(includes: String) =
    """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":$includes}"""

class BundledProductsTest {
    private val manifest = """{"${'$'}v":1,"kind":"worker","entrypoint":"index.js","includes":{"funding":{"routes":[]}}}"""

    private val files = mapOf(
        "bundled-products/provider.dot/manifest.json" to """{"${'$'}v":1,"displayName":"Provider","description":"Card top-ups","icon":{"cid":"bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy","format":"png"}}""",
        "bundled-products/provider.dot/worker-manifest.json" to manifest,
        "bundled-products/provider.dot/worker/index.js" to "export {}",
        "bundled-products/provider.dot/app/index.html" to "<html></html>",
        "bundled-products/provider.dot/app/assets/main.js" to "export {}",
        "bundled-products/chat.dot/worker-manifest.json" to workerManifestJson("""{"chat":true}"""),
        "bundled-products/chat.dot/worker/index.js" to "export {}",
        "bundled-products/quiet.dot/worker-manifest.json" to workerManifestJson("""{"chat":false}"""),
        "bundled-products/quiet.dot/worker/index.js" to "export {}",
        "bundled-products/.provider.dot.staging/worker/index.js" to "ignored",
    )

    private val products = bundledProducts(files)

    private fun entry(id: String) = products.entry(ProductId.parse(id).getOrThrow())

    @Test
    fun `reads each product under the root with its name and icon`() {
        val provider = entry("provider.dot")!!

        assertNull(entry("coinflip.dot"))
        assertEquals(manifest, products.workerManifest(provider))
        assertTrue(provider.hasWorker && provider.hasApp)
        assertEquals("Provider", provider.displayName)
        assertEquals("bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy", provider.iconCid)
        assertNull(entry("chat.dot")!!.displayName)
    }

    @Test
    fun `reads the Worker it ships under its own subname`() {
        val chat = products.resolved(entry("chat.dot")!!).getOrThrow()
        val worker = requireNotNull(chat.executables.worker)

        assertEquals("chat.dot", chat.product.name)
        assertEquals("https://worker.chat.dot/index.js", worker.scriptUrl)
        assertTrue(worker.includesChat)
        assertFalse(products.resolved(entry("quiet.dot")!!).getOrThrow().executables.worker!!.includesChat)
    }

    @Test
    fun `a Worker manifest the parser rejects fails as a published one does`() {
        // provider.dot's manifest has no appVersion.
        assertTrue(products.resolved(entry("provider.dot")!!).isFailure)
    }

    @Test
    fun `opens a bundled product from its files on any network`() {
        val sources = productSources(products)

        assertEquals(ProductSource.Bundled(entry("provider.dot")!!), sources.of("https://provider.dot/card?intent=fs_1"))
        assertNull(sources.of("https://coinflip.paseo/"))
    }

    @Test
    fun `ships nothing without the bundle or in a build that leaves it out`() {
        val provider = ProductId.parse("provider.dot").getOrThrow()

        assertNull(bundledProducts(emptyMap()).entry(provider))
        assertNull(bundledProducts(files, enabled = false).entry(provider))
    }

    @Test
    fun `serves the app's files and routes by the archive rules`() {
        val app = entry("provider.dot")!!.appRoot
        val reader = InMemoryBundledProductFiles(files)

        assertEquals("$app/index.html", archiveFile(app, "/card", true, reader)?.path)
        assertEquals("text/javascript", archiveFile(app, "/assets/main.js", false, reader)?.mime)
        assertNull(archiveFile(app, "/assets/missing.js", false, reader))
    }
}
