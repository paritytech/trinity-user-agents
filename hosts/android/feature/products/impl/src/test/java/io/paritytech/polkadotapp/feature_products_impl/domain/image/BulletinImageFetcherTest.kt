package io.paritytech.polkadotapp.feature_products_impl.domain.image

import coil.ComponentRegistry
import coil.ImageLoader
import coil.annotation.ExperimentalCoilApi
import coil.decode.DataSource
import coil.disk.DiskCache
import coil.fetch.SourceResult
import coil.request.CachePolicy
import coil.request.Options
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.ImageFormat
import uniffi.truapi.ImageReference
import uniffi.truapi.LoadedImage

private val WEBP_BYTES = byteArrayOf(0x52, 0x49, 0x46, 0x46)

@OptIn(ExperimentalCoilApi::class)
class BulletinImageFetcherTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val reference = ImageReference.Bulletin("image-cid", ImageFormat.WEBP)
    private val runtime: TrUAPIHostRuntime = mockk()
    private val runtimeProvider: TrUAPIHostRuntimeProvider = mockk {
        coEvery { runtime() } returns Result.success(runtime)
    }
    private val options: Options = mockk {
        every { context } returns mockk()
        every { diskCachePolicy } returns CachePolicy.ENABLED
    }
    private val directory by lazy { temporaryFolder.newFolder("cache") }
    private val diskCache by lazy { diskCache(directory) }
    private val imageLoader: ImageLoader by lazy { mockk { every { diskCache } returns this@BulletinImageFetcherTest.diskCache } }
    private val factory = BulletinImageFetcher.Factory(runtimeProvider)

    @Test
    fun `immutable bytes survive reopening the disk cache`() = runTest {
        coEvery { runtime.loadImage(reference) } returns LoadedImage(WEBP_BYTES, ImageFormat.WEBP, reference.cid)
        fetch(reference).source.close()
        val restartedDirectory = temporaryFolder.newFolder("restarted")
        directory.copyRecursively(restartedDirectory)
        val restartedCache = diskCache(restartedDirectory)
        val restartedLoader: ImageLoader = mockk { every { diskCache } returns restartedCache }

        for (declaredFormat in listOf(reference.declaredFormat, null)) {
            val cached = factory.create(reference.copy(declaredFormat = declaredFormat), options, restartedLoader).fetch() as SourceResult

            assertEquals(DataSource.DISK to "image/webp", cached.dataSource to cached.mimeType)
            cached.source.use { assertArrayEquals(WEBP_BYTES, it.source().readByteArray()) }
        }
        coVerify(exactly = 1) { runtime.loadImage(any()) }
    }

    @Test
    fun `a changed declaration cannot display cached bytes of another format`() = runTest {
        coEvery { runtime.loadImage(reference) } returns LoadedImage(WEBP_BYTES, ImageFormat.WEBP, reference.cid)
        fetch(reference).source.close()

        val changed = reference.copy(declaredFormat = ImageFormat.PNG)
        val components = ComponentRegistry.Builder().also { BulletinImageLoaderRegistrar(runtimeProvider).register(it) }.build()
        val result = factory.create(changed, options, imageLoader).fetch()

        assertNotEquals(components.key(reference, options), components.key(changed, options))
        assertNull(result)
        coVerify(exactly = 1) { runtime.loadImage(any()) }
    }

    @Test
    fun `cancelling an obsolete image load neither writes nor swallows cancellation`() = runTest {
        val started = CompletableDeferred<Unit>()
        val pending = CompletableDeferred<LoadedImage>()
        coEvery { runtime.loadImage(any()) } coAnswers {
            started.complete(Unit)
            pending.await()
        }
        val loading = async { factory.create(reference, options, imageLoader).fetch() }
        started.await()
        pending.cancel()
        try {
            loading.await()
            error("Cancellation must propagate")
        } catch (error: CancellationException) {
            assertTrue(loading.isCancelled)
        }
        assertNull(diskCache.openSnapshot(reference.cid))
    }

    private suspend fun fetch(reference: ImageReference.Bulletin): SourceResult = factory.create(reference, options, imageLoader).fetch() as SourceResult

    private fun diskCache(directory: java.io.File): DiskCache = DiskCache.Builder()
        .directory(directory)
        .maxSizeBytes(1_024 * 1_024)
        .build()
}
