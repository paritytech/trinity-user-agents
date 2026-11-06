package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import coil.ComponentRegistry
import coil.ImageLoader
import coil.decode.DataSource
import coil.disk.DiskCache
import coil.fetch.SourceResult
import coil.intercept.Interceptor
import coil.request.CachePolicy
import coil.request.ErrorResult
import coil.request.ImageRequest
import coil.request.ImageResult
import coil.request.Options
import coil.request.Parameters
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.design.components.avatar.AVATAR_REFRESH_PARAMETER
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.publicProfileAvatarUri
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.ImageFormat
import uniffi.truapi.ImageReference
import uniffi.truapi.LoadedImage
import uniffi.truapi.ProfileAvatar
import uniffi.truapi.PublicProfile

class PublicProfileAvatarInterceptorTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val avatar = ProfileAvatar("avatar-cid", ImageFormat.WEBP)
    private val runtime: TrUAPIHostRuntime = mockk()
    private val runtimeProvider: TrUAPIHostRuntimeProvider = mockk {
        coEvery { runtime() } returns Result.success(runtime)
    }
    private val components = ComponentRegistry.Builder().also { PublicProfileAvatarImageLoaderRegistrar(runtimeProvider).register(it) }.build()
    private val interceptor = components.interceptors.single()

    @Test
    fun `metadata is resolved before the image cache can answer`() = runTest {
        coEvery { runtime.resolvePublicProfile("alice.01", false) } returns PublicProfile(avatar)
        val chain = chain()

        interceptor.intercept(chain)

        coVerifyOrder {
            runtime.resolvePublicProfile("alice.01", false)
            chain.proceed(any())
        }
        val proceeded = slot<ImageRequest>()
        coVerify { chain.proceed(capture(proceeded)) }
        assertEquals(avatar to avatar.cid, proceeded.captured.data to proceeded.captured.diskCacheKey)
    }

    @Test
    fun `chat refresh forces metadata and retains both image caches`() = runTest {
        coEvery { runtime.resolvePublicProfile("alice.01", true) } returns PublicProfile(avatar)
        val chain = chain(force = true)

        interceptor.intercept(chain)

        coVerify(exactly = 1) { runtime.resolvePublicProfile("alice.01", true) }
        val proceeded = slot<ImageRequest>()
        coVerify { chain.proceed(capture(proceeded)) }
        assertEquals(
            Triple(avatar, CachePolicy.ENABLED, CachePolicy.ENABLED),
            Triple(proceeded.captured.data, proceeded.captured.memoryCachePolicy, proceeded.captured.diskCachePolicy)
        )
        assertEquals(emptyMap<String, String>(), proceeded.captured.parameters.memoryCacheKeys())
    }

    @Test
    fun `profile refreshes reuse bytes and changed or removed avatars take effect`() = runTest {
        val next = ProfileAvatar("next-cid", ImageFormat.PNG)
        coEvery { runtime.resolvePublicProfile(any(), any()) } returnsMany
            listOf(avatar, avatar, next, avatar, null).map { PublicProfile(it) }
        coEvery { runtime.loadImage(any()) } answers {
            val reference = firstArg<ImageReference.Bulletin>()
            LoadedImage(byteArrayOf(1, 2, 3), requireNotNull(reference.declaredFormat), reference.cid)
        }
        val diskCache = DiskCache.Builder().directory(temporaryFolder.newFolder()).maxSizeBytes(1_024 * 1_024).build()
        val imageLoader: ImageLoader = mockk { every { this@mockk.diskCache } returns diskCache }
        val factory = PublicProfileAvatarFetcher.Factory(runtimeProvider)
        val sources = mutableListOf<DataSource>()
        for (force in listOf(false, true, true, false, true)) {
            interceptor.intercept(chain(force = force) { request ->
                val options: Options = mockk {
                    every { context } returns mockk()
                    every { diskCachePolicy } returns request.diskCachePolicy
                }
                val result = factory.create(request.data as ProfileAvatar, options, imageLoader).fetch() as SourceResult
                sources += result.dataSource
                result.source.close()
                mockk()
            })
        }

        assertEquals(listOf(DataSource.NETWORK, DataSource.DISK, DataSource.NETWORK, DataSource.DISK), sources)
        coVerify(exactly = 1) { runtime.loadImage(ImageReference.Bulletin(avatar.cid, avatar.format)) }
        coVerify(exactly = 1) { runtime.loadImage(ImageReference.Bulletin(next.cid, next.format)) }
        coVerify(exactly = 3) { runtime.resolvePublicProfile("alice.01", true) }
    }

    @Test
    fun `a changed profile selects the new CID and declaration before memory lookup`() = runTest {
        val changed = ProfileAvatar("next-cid", ImageFormat.PNG)
        val declarations = listOf(avatar, changed, ProfileAvatar(changed.cid, ImageFormat.JPEG))
        coEvery { runtime.resolvePublicProfile(any(), any()) } returnsMany declarations.map { PublicProfile(it) }
        val keys = mutableListOf<String?>()
        val data = mutableListOf<Any>()
        repeat(declarations.size) {
            interceptor.intercept(chain { request ->
                keys += components.key(request.data, mockk())
                data += request.data
                mockk()
            })
        }

        assertEquals(declarations, data)
        assertEquals(3, keys.toSet().size)
    }

    @Test
    fun `removing an avatar leaves the placeholder without consulting image caches`() = runTest {
        coEvery { runtime.resolvePublicProfile(any(), any()) } returns PublicProfile(null)
        val chain = chain()

        val result = interceptor.intercept(chain)

        assertTrue(result is ErrorResult)
        coVerify(exactly = 0) { chain.proceed(any()) }
    }

    @Test
    fun `cancelling obsolete metadata cannot continue to the image cache`() = runTest {
        val started = CompletableDeferred<Unit>()
        val pending = CompletableDeferred<PublicProfile>()
        coEvery { runtime.resolvePublicProfile(any(), any()) } coAnswers {
            started.complete(Unit)
            pending.await()
        }
        val chain = chain()
        val loading = async { interceptor.intercept(chain) }
        started.await()
        pending.cancel()
        try {
            loading.await()
            error("Cancellation must propagate")
        } catch (error: CancellationException) {
            assertTrue(loading.isCancelled)
        }
        coVerify(exactly = 0) { chain.proceed(any()) }
    }

    @Test
    fun `other images keep their existing request`() = runTest {
        val expected = mockk<ImageResult>()
        val chain = chain(data = "https://example.com/icon.png") { expected }

        assertSame(expected, interceptor.intercept(chain))
        coVerify(exactly = 0) { runtimeProvider.runtime() }
    }

    private fun chain(
        data: Any = publicProfileAvatarUri("alice.01"),
        force: Boolean = false,
        onProceed: suspend (ImageRequest) -> ImageResult = { mockk() },
    ): Interceptor.Chain {
        var resolvedData = data
        var diskKey: String? = null
        val parameters = Parameters.Builder().set(AVATAR_REFRESH_PARAMETER, force, memoryCacheKey = null).build()
        val request: ImageRequest = mockk {
            every { this@mockk.data } answers { resolvedData }
            every { diskCacheKey } answers { diskKey }
            every { this@mockk.parameters } returns parameters
            every { memoryCachePolicy } returns CachePolicy.ENABLED
            every { diskCachePolicy } returns CachePolicy.ENABLED
            every { error } returns null
        }
        val builder: ImageRequest.Builder = mockk {
            every { data(any()) } answers { resolvedData = firstArg(); this@mockk }
            every { diskCacheKey(any()) } answers { diskKey = firstArg(); this@mockk }
            every { build() } returns request
        }
        every { request.newBuilder() } returns builder
        return mockk {
            every { this@mockk.request } returns request
            coEvery { proceed(request) } coAnswers { onProceed(request) }
        }
    }
}
