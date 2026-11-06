package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import android.net.Uri
import coil.annotation.ExperimentalCoilApi
import coil.decode.DataSource
import coil.disk.DiskCache
import coil.fetch.SourceResult
import coil.memory.MemoryCache
import coil.request.CachePolicy
import coil.request.Options
import coil.request.Parameters
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.publicProfileAvatarUri
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.ImageException
import uniffi.truapi.ImageFormat
import uniffi.truapi.ImageReference
import uniffi.truapi.LoadedImage

private val WEBP_BYTES = byteArrayOf(0x52, 0x49, 0x46, 0x46)

@OptIn(ExperimentalCoilApi::class)
class PublicProfileAvatarFetcherTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val runtime: TrUAPIHostRuntime = mockk()
    private val runtimeProvider: TrUAPIHostRuntimeProvider = mockk {
        coEvery { runtime() } returns Result.success(runtime)
    }
    private val options: Options = mockk {
        every { context } returns mockk()
        every { diskCachePolicy } returns CachePolicy.ENABLED
        every { diskCacheKey } returns null
        every { parameters } returns Parameters.EMPTY
    }
    private var now = 1_000L
    private val memoryCache: MemoryCache = mockk(relaxed = true)
    private val diskCache by lazy { diskCache(temporaryFolder.newFolder("cache")) }
    private val cache by lazy { PublicProfileAvatarCache(diskCache, memoryCache) { now } }
    private val factory by lazy { PublicProfileAvatarFetcher.Factory(runtimeProvider, cache) }

    @Test
    fun `a fresh avatar survives restarting the image loader`() = runTest {
        coEvery { runtime.loadImage(ImageReference.Username("alice.01", force = false)) } returns
            LoadedImage(WEBP_BYTES, ImageFormat.WEBP, "avatar-cid")
        val uri = uriOf(publicProfileAvatarUri("alice.01"))
        val loaded = factory.create(uri, options, mockk())?.fetch() as SourceResult
        loaded.source.close()
        val restartedDirectory = temporaryFolder.newFolder("restarted")
        diskCache.directory.toFile().copyRecursively(restartedDirectory)
        now += 3_599_999
        val restartedCache = PublicProfileAvatarCache(diskCache(restartedDirectory), memoryCache) { now }
        val restartedFactory = PublicProfileAvatarFetcher.Factory(runtimeProvider, restartedCache)

        val cached = restartedFactory.create(uri, options, mockk())?.fetch() as SourceResult

        assertEquals(DataSource.DISK, cached.dataSource)
        cached.source.use { assertArrayEquals(WEBP_BYTES, it.source().readByteArray()) }
        coVerify(exactly = 1) { runtime.loadImage(ImageReference.Username("alice.01", force = false)) }
        now += 1
        val renewed = restartedFactory.create(uri, options, mockk())?.fetch() as SourceResult
        renewed.source.close()
        assertEquals(DataSource.NETWORK, renewed.dataSource)
        coVerify(exactly = 2) { runtime.loadImage(ImageReference.Username("alice.01", force = false)) }
    }

    @Test
    fun `reading a cached avatar does not postpone its expiry`() = runTest {
        val nextBytes = byteArrayOf(1, 2, 3)
        coEvery { runtime.loadImage(ImageReference.Username("alice.01", force = false)) } returnsMany listOf(
            LoadedImage(WEBP_BYTES, ImageFormat.WEBP, "first-cid"),
            LoadedImage(nextBytes, ImageFormat.PNG, "next-cid"),
        )
        val uri = uriOf(publicProfileAvatarUri("alice.01"))
        val loaded = factory.create(uri, options, mockk())?.fetch() as SourceResult
        loaded.source.close()
        now += 3_599_999
        val cached = factory.create(uri, options, mockk())?.fetch() as SourceResult
        cached.source.close()
        now += 1

        val renewed = factory.create(uri, options, mockk())?.fetch() as SourceResult

        assertEquals(listOf(DataSource.NETWORK, DataSource.DISK, DataSource.NETWORK), listOf(loaded.dataSource, cached.dataSource, renewed.dataSource))
        renewed.source.use { assertArrayEquals(nextBytes, it.source().readByteArray()) }
        coVerify(exactly = 2) { runtime.loadImage(ImageReference.Username("alice.01", force = false)) }
    }

    // The core verifies the bytes against the record's CID; its detected format selects Coil's decoder.
    @Test
    fun `a contact's avatar is the image the core resolves for their username`() = runTest {
        coEvery { runtime.loadImage(ImageReference.Username("alice.01", force = false)) } returns
            LoadedImage(WEBP_BYTES, ImageFormat.WEBP, "avatar-cid")

        val result = factory.create(uriOf(publicProfileAvatarUri("alice.01")), options, mockk())?.fetch() as SourceResult

        assertEquals("image/webp", result.mimeType)
        assertArrayEquals(WEBP_BYTES, result.source.source().readByteArray())
    }

    // An opened chat refreshes its avatar past Coil's caches, so the core's cached record must not answer either.
    @Test
    fun `a refresh that skips the disk cache rereads the profile`() = runTest {
        cache.write(publicProfileAvatarUri("alice.01"), byteArrayOf(9), "image/png", options)
        every { options.diskCachePolicy } returns CachePolicy.WRITE_ONLY
        coEvery { runtime.loadImage(ImageReference.Username("alice.01", force = true)) } returns
            LoadedImage(WEBP_BYTES, ImageFormat.WEBP, "avatar-cid")

        val result = factory.create(uriOf(publicProfileAvatarUri("alice.01")), options, mockk())?.fetch() as SourceResult

        result.source.use { assertArrayEquals(WEBP_BYTES, it.source().readByteArray()) }
        val stored = requireNotNull(cache.read(publicProfileAvatarUri("alice.01"), options))
        stored.source.use { assertArrayEquals(WEBP_BYTES, it.source().readByteArray()) }
        coVerify(exactly = 1) { runtime.loadImage(ImageReference.Username("alice.01", force = true)) }
    }

    // No fetch result means Coil draws the error image, which for an avatar is the contact's letter.
    @Test
    fun `a username without an avatar leaves the letter drawn`() = runTest {
        coEvery { runtime.loadImage(ImageReference.Username("bob.02", force = false)) } throws ImageException.Unavailable("no avatar")

        assertNull(factory.create(uriOf(publicProfileAvatarUri("bob.02")), options, mockk())?.fetch())
        coVerify(exactly = 1) { runtime.loadImage(ImageReference.Username("bob.02", force = false)) }
    }

    @Test
    fun `other images are left to the fetchers that serve them`() {
        assertNull(factory.create(uriOf("https://example.com/icon.png"), options, mockk()))
    }

    private fun diskCache(directory: java.io.File): DiskCache = DiskCache.Builder()
        .directory(directory)
        .maxSizeBytes(1_024 * 1_024)
        .build()

    // android.net.Uri has no JVM implementation in unit tests; this splits an opaque URI the way it does.
    private fun uriOf(value: String): Uri = mockk {
        every { scheme } returns value.substringBefore(':')
        every { schemeSpecificPart } returns value.substringAfter(':')
    }
}
