package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.FundingProviderEntry
import uniffi.truapi.FundingProviderOrigin
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

class BundledProductsTest {
    private val manifest = """{"${'$'}v":1,"kind":"worker","entrypoint":"index.js","includes":{"funding":{"routes":[]}}}"""

    private val files = InMemoryBundledProductFiles(
        mapOf(
            "bundled-products/provider.dot/worker-manifest.json" to manifest,
            "bundled-products/provider.dot/worker/index.js" to "export {}",
            "bundled-products/provider.dot/app/index.html" to "<html></html>",
            "bundled-products/provider.dot/app/assets/main.js" to "export {}",
            "bundled-products/chat.dot/worker-manifest.json" to """{"${'$'}v":1,"kind":"worker","includes":{"chat":true}}""",
            "bundled-products/chat.dot/worker/index.js" to "export {}",
            "bundled-products/.provider.dot.staging/worker/index.js" to "ignored",
        ),
    )

    private val products = BundledProducts(files, enabled = true)

    @Test
    fun `reads each product under the root`() {
        val product = products.product(ProductId.fromStoredValue("provider.dot"))!!

        assertEquals(listOf("chat.dot", "provider.dot"), products.all.map { it.id.value })
        assertEquals(manifest, products.workerManifest(product))
        assertTrue(products.hasWorker(product))
        assertTrue(products.hasApp(product))
        assertEquals("https://worker.provider.dot/index.js", product.workerScriptUrl)
    }

    @Test
    fun `lists only the products that serve funding as bundled providers`() {
        assertEquals(
            listOf(FundingProviderEntry(productId = "provider.dot", workerManifest = manifest, origin = FundingProviderOrigin.BUNDLED)),
            products.fundingProviders(),
        )
    }

    @Test
    fun `serves a request the way a published archive is served`() {
        val app = products.product("provider.dot")!!.appRoot

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
    fun `ships nothing without the bundle or outside testnet builds`() {
        assertTrue(BundledProducts(InMemoryBundledProductFiles(emptyMap()), enabled = true).all.isEmpty())
        assertTrue(BundledProducts(files, enabled = false).all.isEmpty())
        assertTrue(BundledProducts(files, enabled = false).fundingProviders().isEmpty())
    }
}
