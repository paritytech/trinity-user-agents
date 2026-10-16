package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.HandInstalledWorkers
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class TrUAPIWorkerCodeSourceTest {
    @get:Rule
    val files = TemporaryFolder()

    private val archive = mockk<ProductWorkerArchive>()

    private val products by lazy { files.newFolder("Products") }

    private fun source() = TrUAPIWorkerCodeSource(HandInstalledWorkers(products), archive)

    private fun worker(scriptUrl: String) = mockk<ProductExecutable.Worker> { every { this@mockk.scriptUrl } returns scriptUrl }

    private fun handInstalled(name: String, bytes: ByteArray): File =
        File(products, "${PRODUCT.value}/ChatExtension/$name").apply {
            parentFile?.mkdirs()
            writeBytes(bytes)
        }

    private fun withPublishedEntry(bytes: ByteArray) {
        val entry = files.newFile("index").apply { writeBytes(bytes) }
        coEvery { archive.file(PRODUCT, "index.wasm") } returns Result.success(entry)
    }

    @Test
    fun `a published entry that starts with the wasm magic is a wasm module`() = runBlocking {
        withPublishedEntry(WASM)

        val code = source().load(PRODUCT, worker("https://worker.verbosity.dot/index.wasm")).getOrThrow()

        assertTrue(code is TrUAPIWorkerCode.Wasm)
        assertArrayEquals(WASM, (code as TrUAPIWorkerCode.Wasm).module)
        assertEquals("verbosity.dot/index.wasm", code.name)
    }

    @Test
    fun `a published entry with any other bytes stays a script`() = runBlocking {
        withPublishedEntry("export {};".toByteArray())

        val code = source().load(PRODUCT, worker("https://worker.verbosity.dot/index.wasm")).getOrThrow()

        assertEquals(TrUAPIWorkerCode.Script("https://worker.verbosity.dot/index.wasm"), code)
    }

    @Test
    fun `a worker on the loopback address stays a script and reads no archive`() = runBlocking {
        val code = source().load(PRODUCT, worker("http://127.0.0.1:3100/worker/index.js")).getOrThrow()

        assertEquals(TrUAPIWorkerCode.Script("http://127.0.0.1:3100/worker/index.js"), code)
        coVerify(exactly = 0) { archive.file(any(), any()) }
    }

    @Test
    fun `a worker installed by hand runs from its file`() = runBlocking {
        val entry = handInstalled("index.wasm", WASM)

        val code = source().load(PRODUCT, worker(entry.toURI().toString())).getOrThrow()

        assertArrayEquals(WASM, (code as TrUAPIWorkerCode.Wasm).module)
        assertEquals("verbosity.dot/ChatExtension/index.wasm", code.name)
    }

    // The WebView cannot read a local file, so a script installed by hand cannot run.
    @Test
    fun `a script installed by hand is refused`() = runBlocking {
        val entry = handInstalled("index.js", "export {};".toByteArray())

        assertTrue(source().load(PRODUCT, worker(entry.toURI().toString())).isFailure)
    }

    @Test
    fun `a file outside the folder of the products is no worker installed by hand`() {
        val outside = files.newFile("index.wasm").apply { writeBytes(WASM) }

        assertNull(HandInstalledWorkers(products).entryFile(outside.toURI().toString()))
    }

    @Test
    fun `the manifest beside a worker installed by hand is read`() {
        val entry = handInstalled("index.wasm", WASM)
        File(entry.parentFile, "manifest.json").writeText("""{"keepAlive":"foreground"}""")

        assertEquals("""{"keepAlive":"foreground"}""", HandInstalledWorkers(products).manifestBeside(entry))
    }

    @Test
    fun `only the wasm magic marks a module`() {
        assertTrue(TrUAPIWorkerCode.isWasm(WASM))
        assertTrue(!TrUAPIWorkerCode.isWasm(byteArrayOf(0x00, 0x61, 0x73)))
        assertTrue(!TrUAPIWorkerCode.isWasm("\u0000asn".toByteArray()))
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("verbosity.dot")
        val WASM = byteArrayOf(0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00)
    }
}
