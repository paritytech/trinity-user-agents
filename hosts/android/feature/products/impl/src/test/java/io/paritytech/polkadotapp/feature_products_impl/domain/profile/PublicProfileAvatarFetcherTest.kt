package io.paritytech.polkadotapp.feature_products_impl.domain.profile

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
import uniffi.truapi.ProfileAvatar

private val WEBP_BYTES = byteArrayOf(0x52, 0x49, 0x46, 0x46)

@OptIn(ExperimentalCoilApi::class)
class PublicProfileAvatarFetcherTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val avatar = ProfileAvatar("avatar-cid", ImageFormat.WEBP)
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
    private val imageLoader: ImageLoader by lazy { mockk { every { diskCache } returns this@PublicProfileAvatarFetcherTest.diskCache } }
    private val factory = PublicProfileAvatarFetcher.Factory(runtimeProvider)

    @Test
    fun `immutable bytes survive reopening the disk cache`() = runTest {
        coEvery { runtime.loadImage(ImageReference.Bulletin(avatar.cid, avatar.format)) } returns
            LoadedImage(WEBP_BYTES, avatar.format, avatar.cid)
        fetch(avatar).source.close()
        val restartedDirectory = temporaryFolder.newFolder("restarted")
        directory.copyRecursively(restartedDirectory)
        val restartedCache = diskCache(restartedDirectory)
        val restartedLoader: ImageLoader = mockk { every { diskCache } returns restartedCache }

        val cached = factory.create(avatar, options, restartedLoader).fetch() as SourceResult

        assertEquals(DataSource.DISK, cached.dataSource)
        cached.source.use { assertArrayEquals(WEBP_BYTES, it.source().readByteArray()) }
        coVerify(exactly = 1) { runtime.loadImage(any()) }
    }

    @Test
    fun `a changed declaration cannot display cached bytes of another format`() = runTest {
        coEvery { runtime.loadImage(ImageReference.Bulletin(avatar.cid, avatar.format)) } returns
            LoadedImage(WEBP_BYTES, avatar.format, avatar.cid)
        fetch(avatar).source.close()

        val changed = ProfileAvatar(avatar.cid, ImageFormat.PNG)
        val components = ComponentRegistry.Builder().also { PublicProfileAvatarImageLoaderRegistrar(runtimeProvider).register(it) }.build()
        val result = factory.create(changed, options, imageLoader).fetch()

        assertNotEquals(components.key(avatar, options), components.key(changed, options))
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
        val loading = async { factory.create(avatar, options, imageLoader).fetch() }
        started.await()
        pending.cancel()
        try {
            loading.await()
            error("Cancellation must propagate")
        } catch (error: CancellationException) {
            assertTrue(loading.isCancelled)
        }
        assertNull(diskCache.openSnapshot(avatar.cid))
    }

    private suspend fun fetch(profile: ProfileAvatar): SourceResult = factory.create(profile, options, imageLoader).fetch() as SourceResult

    private fun diskCache(directory: java.io.File): DiskCache = DiskCache.Builder()
        .directory(directory)
        .maxSizeBytes(1_024 * 1_024)
        .build()
}
