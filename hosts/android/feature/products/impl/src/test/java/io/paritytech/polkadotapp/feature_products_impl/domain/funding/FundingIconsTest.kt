package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import android.net.Uri
import coil.ImageLoader
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.request.Options
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.ProductId
import java.io.File

class FundingIconsTest {
    @get:Rule
    val archives = TemporaryFolder()

    private val ramp = ProductId.parse("ramp.paseo").getOrThrow()
    private val other = ProductId.parse("other.paseo").getOrThrow()

    private val dotNsResolver: DotNsResolver = mockk()
    private val dotNsTldProvider: DotNsTldProvider = mockk {
        coEvery { getTld() } returns Result.success(requireNotNull(DotNsTld.parse("paseo")))
    }
    private val files = FundingIconFiles(ProductWorkerArchive(dotNsResolver))
    private val factory = FundingIconFetcher.Factory(dotNsTldProvider, files)
    private val options: Options = mockk()
    private val imageLoader: ImageLoader = mockk()

    @Test
    fun `an icon is named by its lowercased id`() {
        assertEquals("funding/icons/networks/polkadot.png", FundingIconPath.of(FundingIconKind.NETWORK, "Polkadot"))
        assertEquals("funding/icons/tokens/usdc.png", FundingIconPath.of(FundingIconKind.TOKEN, "USDC"))
        assertEquals("funding/icons/networks/bnb-chain2.png", FundingIconPath.of(FundingIconKind.NETWORK, "bnb-chain2"))
    }

    @Test
    fun `an id outside a-z, 0-9 and dash names no icon`() {
        listOf("", "usd.c", "../eth", "arb_one", "ÜSD", "a/b").forEach { id ->
            assertNull(id, FundingIconPath.of(FundingIconKind.TOKEN, id))
            assertNull(id, fundingIconModel(ramp, FundingIconKind.TOKEN, id))
        }
    }

    @Test
    fun `the icon is loaded from the provider's worker archive`() = runTest {
        val icon = publish(ramp, "funding/icons/tokens/usdc.png")
        val loaded = withFileImage(icon)

        assertSame(loaded, fetch(fundingIconModel(ramp, FundingIconKind.TOKEN, "USDC")))
    }

    @Test
    fun `an icon the archive does not ship fails, leaving the monogram`() = runTest {
        publish(ramp, "funding/icons/tokens/usdc.png")

        val loaded = runCatching { fetch(fundingIconModel(ramp, FundingIconKind.TOKEN, "usdt")) }

        assertTrue(loaded.isFailure)
    }

    @Test
    fun `another provider's icon is never used`() = runTest {
        publish(ramp, "funding/icons/networks/polkadot.png")
        publish(other)

        val loaded = runCatching { fetch(fundingIconModel(other, FundingIconKind.NETWORK, "polkadot")) }

        assertTrue(loaded.isFailure)
        coVerify(exactly = 0) { dotNsResolver.resolveToLocalUri("worker.ramp.paseo") }
    }

    // Anything can build an image model, so a name that is not a product of this network is never resolved.
    @Test
    fun `a provider outside this network's products is not resolved`() = runTest {
        val loaded = runCatching { fetch("funding-icon:evil.dot/tokens/usdc") }

        assertTrue(loaded.isFailure)
        coVerify(exactly = 0) { dotNsResolver.resolveToLocalUri(any()) }
    }

    @Test
    fun `other images are left to the fetchers that serve them`() {
        assertNull(factory.create(uriOf("https://example.com/icon.png"), options, imageLoader))
    }

    private suspend fun fetch(model: String?): FetchResult? =
        requireNotNull(factory.create(uriOf(requireNotNull(model)), options, imageLoader)).fetch()

    /** Unpacks [provider]'s worker archive with [paths] in it, as dotNS serves it locally. */
    private fun publish(provider: ProductId, vararg paths: String): File {
        val root = archives.newFolder(provider.value)
        paths.forEach { path -> File(root, path).apply { parentFile?.mkdirs() }.writeText("png") }
        val uri: Uri = mockk { every { path } returns root.path }
        coEvery { dotNsResolver.resolveToLocalUri("worker.${provider.value}") } returns Result.success(uri)

        return File(root, paths.firstOrNull().orEmpty())
    }

    private fun withFileImage(file: File): FetchResult {
        val result = mockk<FetchResult>()
        val fileFetcher = mockk<Fetcher> { coEvery { fetch() } returns result }
        every { imageLoader.components } returns mockk {
            every { map(any(), options) } answers { firstArg() }
            every { newFetcher(match { it is File && it.canonicalPath == file.canonicalPath }, options, imageLoader) } returns (fileFetcher to 0)
        }
        return result
    }

    // android.net.Uri has no JVM implementation in unit tests; this splits an opaque URI the way it does.
    private fun uriOf(value: String): Uri = mockk {
        every { scheme } returns value.substringBefore(':')
        every { schemeSpecificPart } returns value.substringAfter(':')
    }
}
