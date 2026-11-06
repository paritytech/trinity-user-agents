package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import android.graphics.Bitmap
import android.graphics.drawable.BitmapDrawable
import coil.decode.DataSource
import coil.disk.DiskCache
import coil.intercept.Interceptor
import coil.memory.MemoryCache
import coil.request.ErrorResult
import coil.request.ImageRequest
import coil.request.ImageResult
import coil.request.Options
import coil.request.Parameters
import coil.request.SuccessResult
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.publicProfileAvatarUri
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class PublicProfileAvatarCacheTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val uri = publicProfileAvatarUri("alice.01")
    private val key = MemoryCache.Key(uri)
    private val entries = mutableMapOf<MemoryCache.Key, MemoryCache.Value>()
    private val memoryCache: MemoryCache = mockk {
        every { keys } answers { entries.keys.toSet() }
        every { this@mockk.get(any<MemoryCache.Key>()) } answers { entries[firstArg()] }
        every { set(any(), any()) } answers { entries[firstArg()] = secondArg() }
        every { remove(any()) } answers { entries.remove(firstArg()) != null }
    }
    private var now = 1_000L
    private val diskCache by lazy {
        DiskCache.Builder().directory(temporaryFolder.newFolder()).maxSizeBytes(1_024 * 1_024).build()
    }
    private val cache by lazy { PublicProfileAvatarCache(diskCache, memoryCache) { now } }
    private val options: Options = mockk { every { parameters } returns Parameters.EMPTY }

    @Test
    fun `disk promotion and memory hits retain the original expiry`() = runTest {
        cache.write(uri, byteArrayOf(1, 2, 3), "image/png", options)
        now += 3_000_000
        val diskHit = memoryWasAvailable(DataSource.DISK)
        now += 300_000
        val memoryHit = memoryWasAvailable(DataSource.MEMORY_CACHE)
        now += 300_000
        val expired = memoryWasAvailable(null)

        assertEquals(listOf(false, true, false), listOf(diskHit, memoryHit, expired))
    }

    @Test
    fun `a pending disk image does not inherit a concurrent refresh's later expiry`() = runTest {
        cache.write(uri, byteArrayOf(1), "image/png", options)
        now += 3_000_000
        cache.intercept(chain { requestOptions ->
            cache.read(uri, requestOptions)?.source?.close()
            now += 300_000
            cache.write(uri, byteArrayOf(2), "image/png", options)
            val bitmap = mockk<Bitmap>()
            entries[key] = MemoryCache.Value(bitmap)
            result(DataSource.DISK, bitmap)
        })
        now += 300_000

        assertEquals(false, memoryWasAvailable(null))
    }

    @Test
    fun `a finishing request does not overwrite another request's memory entry`() = runTest {
        cache.write(uri, byteArrayOf(1), "image/png", options)
        val newerEntry = MemoryCache.Value(mockk(), mapOf("marker" to "newer"))
        cache.intercept(chain { requestOptions ->
            cache.read(uri, requestOptions)?.source?.close()
            entries[key] = newerEntry
            result(DataSource.DISK, mockk())
        })

        assertSame(newerEntry, entries[key])
    }

    private suspend fun memoryWasAvailable(source: DataSource?): Boolean {
        var available = false
        cache.intercept(chain { requestOptions ->
            available = entries.containsKey(key)
            if (source == null) {
                mockk<ErrorResult>()
            } else {
                val bitmap = if (source == DataSource.DISK) {
                    cache.read(uri, requestOptions)?.source?.close()
                    mockk<Bitmap>().also { entries[key] = MemoryCache.Value(it) }
                } else {
                    requireNotNull(entries[key]).bitmap
                }
                result(source, bitmap)
            }
        })
        return available
    }

    private fun result(source: DataSource, bitmap: Bitmap): SuccessResult {
        val drawable = mockk<BitmapDrawable> { every { this@mockk.bitmap } returns bitmap }
        return mockk {
            every { dataSource } returns source
            every { memoryCacheKey } returns key
            every { this@mockk.drawable } returns drawable
        }
    }

    private fun chain(proceed: (Options) -> ImageResult): Interceptor.Chain {
        val parameters = Parameters.Builder()
        val request: ImageRequest = mockk {
            every { data } returns uri
            every { context } returns mockk()
            every { memoryCacheKey } returns null
            every { this@mockk.parameters } answers { parameters.build() }
        }
        val builder: ImageRequest.Builder = mockk {
            every { setParameter(any(), any(), any()) } answers {
                parameters.set(firstArg(), secondArg(), thirdArg())
                this@mockk
            }
            every { build() } returns request
        }
        every { request.newBuilder() } returns builder
        val requestOptions: Options = mockk { every { this@mockk.parameters } answers { request.parameters } }
        return mockk {
            every { this@mockk.request } returns request
            coEvery { proceed(request) } answers {
                assertEquals(emptyMap<String, String>(), request.parameters.memoryCacheKeys())
                proceed(requestOptions)
            }
        }
    }
}
