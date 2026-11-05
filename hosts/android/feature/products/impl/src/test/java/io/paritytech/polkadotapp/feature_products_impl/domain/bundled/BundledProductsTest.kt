package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import com.google.gson.Gson
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId
import java.io.ByteArrayInputStream
import java.io.InputStream

/** Asset files by path, listed the way the asset manager lists them: a directory's children, a file's none. */
class InMemoryBundledProductFiles(private val files: Map<String, String>) : BundledProductFiles {
    override fun list(path: String): List<String> {
        val prefix = "$path/"
        return files.keys
            .filter { it.startsWith(prefix) }
            .map { it.removePrefix(prefix).substringBefore('/') }
            .distinct()
    }

    override fun open(path: String): InputStream? = files[path]?.let { ByteArrayInputStream(it.toByteArray()) }
}

private fun workerManifestJson(includes: String) =
    """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":$includes}"""

class BundledProductsTest {
    private val manifest = """{"${'$'}v":1,"kind":"worker","entrypoint":"index.js","includes":{"funding":{"routes":[]}}}"""

    private val files = InMemoryBundledProductFiles(
        mapOf(
            "bundled-products/provider.dot/worker-manifest.json" to manifest,
            "bundled-products/provider.dot/worker/index.js" to "export {}",
            "bundled-products/provider.dot/app/index.html" to "<html></html>",
            "bundled-products/provider.dot/app/assets/main.js" to "export {}",
            "bundled-products/chat.dot/worker-manifest.json" to workerManifestJson("""{"chat":true}"""),
            "bundled-products/chat.dot/worker/index.js" to "export {}",
            "bundled-products/quiet.dot/worker-manifest.json" to workerManifestJson("""{"chat":false}"""),
            "bundled-products/quiet.dot/worker/index.js" to "export {}",
            "bundled-products/.provider.dot.staging/worker/index.js" to "ignored",
        ),
    )

    private val products = BundledProducts(files, ManifestParser(Gson()), enabled = true)

    @Test
    fun `reads each product under the root`() {
        val product = products.product(ProductId.parse("provider.dot").getOrThrow())!!

        assertNotNull(products.product(ProductId.parse("chat.dot").getOrThrow()))
        assertNull(products.product(ProductId.parse("coinflip.dot").getOrThrow()))
        assertEquals(manifest, products.workerManifest(product))
        assertTrue(products.hasWorker(product))
        assertTrue(products.hasApp(product))
        assertEquals("https://worker.provider.dot/index.js", product.workerScriptUrl)
    }

    @Test
    fun `includes chat only when the manifest sets it`() {
        assertTrue(products.includesChat(products.product(ProductId.parse("chat.dot").getOrThrow())!!))
        assertFalse(products.includesChat(products.product(ProductId.parse("quiet.dot").getOrThrow())!!))
        assertFalse(products.includesChat(products.product(ProductId.parse("provider.dot").getOrThrow())!!))
    }

    @Test
    fun `serves a request the way a published archive is served`() {
        val app = products.product(ProductId.parse("provider.dot").getOrThrow())!!.appRoot

        assertEquals("$app/index.html", products.assetPath(app, "/", isMainFrame = true))
        assertEquals("$app/assets/main.js", products.assetPath(app, "/assets/main.js", isMainFrame = false))
        assertEquals("$app/index.html", products.assetPath(app, "/card", isMainFrame = true))
        assertNull(products.assetPath(app, "/assets/missing.js", isMainFrame = false))
        assertNull(products.assetPath(app, "/../worker/index.js", isMainFrame = false))
    }

    @Test
    fun `opens a provider's screens under its own id on any network`() {
        assertEquals("provider.dot", products.appFor("https://provider.dot/card?intent=fs_1")?.id?.value)
        assertNull(products.appFor("https://chat.dot/"))
        assertNull(products.appFor("https://coinflip.paseo/"))
    }

    @Test
    fun `ships nothing without the bundle or in a build that leaves it out`() {
        val provider = ProductId.parse("provider.dot").getOrThrow()

        val empty = InMemoryBundledProductFiles(emptyMap())
        assertNull(BundledProducts(empty, ManifestParser(Gson()), enabled = true).product(provider))
        assertNull(BundledProducts(files, ManifestParser(Gson()), enabled = false).product(provider))
    }
}

class BundledProductContentTest {
    private val files = InMemoryBundledProductFiles(
        mapOf(
            "bundled-products/provider.dot/worker/index.js" to "export {}",
            "bundled-products/provider.dot/app/index.html" to "<html></html>",
            "bundled-products/provider.dot/app/assets/main.js" to "export {}",
            "bundled-products/provider.dot/app/assets/core.wasm" to "wasm",
        ),
    )
    private val products = BundledProducts(files, ManifestParser(Gson()), enabled = true)
    private val product = products.product(ProductId.parse("provider.dot").getOrThrow())!!
    private val headers = mapOf("Content-Security-Policy" to "frame-ancestors *")
    private val app = BundledProductContent.app(product, products, headers)

    @Test
    fun `leaves another host to the other clients`() {
        assertNull(app.answer("coinflip.dot", "/", isMainFrame = true))
        assertNull(app.answer("worker.provider.dot", "/index.js", isMainFrame = false))
        assertNull(app.answer(null, "/", isMainFrame = true))
    }

    @Test
    fun `answers its own host whatever the case`() {
        assertEquals(
            BundledProductContent.Answer.File("${product.appRoot}/index.html", "text/html", headers),
            app.answer("Provider.DOT", "/", isMainFrame = true),
        )
    }

    @Test
    fun `a missing file is not found`() {
        assertEquals(BundledProductContent.Answer.NotFound, app.answer("provider.dot", "/assets/missing.js", isMainFrame = false))
    }

    @Test
    fun `serves module scripts and wasm with the types that run`() {
        assertEquals(
            BundledProductContent.Answer.File("${product.appRoot}/assets/main.js", "text/javascript", emptyMap()),
            app.answer("provider.dot", "/assets/main.js", isMainFrame = false),
        )
        assertEquals(
            "application/wasm",
            (app.answer("provider.dot", "/assets/core.wasm", isMainFrame = false) as BundledProductContent.Answer.File).mimeType,
        )
    }

    @Test
    fun `only the main document carries the frame headers`() {
        val route = app.answer("provider.dot", "/card", isMainFrame = true) as BundledProductContent.Answer.File
        val script = app.answer("provider.dot", "/assets/main.js", isMainFrame = false) as BundledProductContent.Answer.File

        assertEquals(headers, route.headers)
        assertEquals(emptyMap<String, String>(), script.headers)
    }

    @Test
    fun `serves the worker under its own subname`() {
        val worker = BundledProductContent.worker(product, products)

        assertEquals(
            BundledProductContent.Answer.File("${product.workerRoot}/index.js", "text/javascript", emptyMap()),
            worker.answer("worker.provider.dot", "/index.js", isMainFrame = false),
        )
        assertNull(worker.answer("provider.dot", "/index.js", isMainFrame = false))
    }
}
