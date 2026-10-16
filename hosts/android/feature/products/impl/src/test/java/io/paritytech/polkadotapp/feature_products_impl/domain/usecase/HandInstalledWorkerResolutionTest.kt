package io.paritytech.polkadotapp.feature_products_impl.domain.usecase

import com.google.gson.Gson
import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.WorkerKeepAlive
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.product.HandInstalledWorkers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class HandInstalledWorkerResolutionTest {
    @get:Rule
    val files = TemporaryFolder()

    private val productId = ProductId.fromStoredValue("verbosity.dot")
    private val products by lazy { files.newFolder("Products") }
    private val entry by lazy {
        File(products, "verbosity.dot/ChatExtension/index.wasm").apply {
            parentFile?.mkdirs()
            writeBytes(byteArrayOf(0x00, 0x61, 0x73, 0x6D))
        }
    }

    private fun resolveWorkerAt(url: String) = runBlocking {
        val dotNs = mockk<DotNsResolver> { coEvery { getMetadataEntry("verbosity.dot", "manifest") } returns Result.success(null) }
        val repository = mockk<ProductRepository>(relaxed = true) { coEvery { getUserWorkerUrl(productId) } returns url }
        val cards = mockk<DebugPocketCards> { coEvery { get(productId) } returns null }
        RealResolveProductUseCase(dotNs, ManifestParser(Gson()), repository, cards, HandInstalledWorkers(products))
            .resolve(productId)
            .getOrThrow()
            .executables
            .worker
    }

    private fun withManifest(text: String) = File(entry.parentFile, "manifest.json").writeText(text)

    @Test
    fun `a worker installed by hand takes the keepAlive of the manifest beside it`() {
        withManifest("""{"${'$'}v":1,"kind":"worker","entrypoint":"index.wasm","keepAlive":"foreground"}""")

        val worker = resolveWorkerAt(entry.toURI().toString())

        assertEquals(entry.toURI().toString(), worker?.scriptUrl)
        assertEquals(true, worker?.includesChat)
        assertEquals(WorkerKeepAlive.FOREGROUND, worker?.keepAlive)
    }

    @Test
    fun `a worker installed by hand with no manifest runs on demand`() {
        assertEquals(WorkerKeepAlive.ON_DEMAND, resolveWorkerAt(entry.toURI().toString())?.keepAlive)
    }

    @Test
    fun `a worker on the loopback address runs on demand`() {
        withManifest("""{"keepAlive":"foreground"}""")

        assertEquals(WorkerKeepAlive.ON_DEMAND, resolveWorkerAt("http://127.0.0.1:3100/worker/index.js")?.keepAlive)
    }
}
