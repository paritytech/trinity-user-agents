package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import android.graphics.drawable.BitmapDrawable
import coil.annotation.ExperimentalCoilApi
import coil.decode.DataSource
import coil.decode.ImageSource
import coil.disk.DiskCache
import coil.fetch.SourceResult
import coil.intercept.Interceptor
import coil.memory.MemoryCache
import coil.request.ImageResult
import coil.request.Options
import coil.request.SuccessResult
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.isPublicProfileAvatarUri
import java.util.concurrent.atomic.AtomicLong
import kotlin.time.Duration.Companion.hours

@OptIn(ExperimentalCoilApi::class)
internal class PublicProfileAvatarCache(
    private val diskCache: DiskCache,
    private val memoryCache: MemoryCache,
    private val now: () -> Long = System::currentTimeMillis,
) : Interceptor {
    override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
        val request = chain.request
        val uri = request.data.toString()
        if (!isPublicProfileAvatarUri(uri)) return chain.proceed(request)

        val baseKey = request.memoryCacheKey?.key ?: uri
        memoryCache.keys.filter { it.key == baseKey }.forEach { key ->
            val expiresAt = memoryCache[key]?.extras?.get(EXPIRES_AT) as? Long
            if (expiresAt == null || expiresAt <= now()) memoryCache.remove(key)
        }

        val expiresAt = AtomicLong()
        val result = chain.proceed(request.newBuilder().setParameter(EXPIRES_AT, expiresAt, memoryCacheKey = null).build())
        if (result is SuccessResult && result.dataSource != DataSource.MEMORY_CACHE) {
            val key = result.memoryCacheKey ?: return result
            val cached = memoryCache[key]
            if (cached != null && cached.bitmap === (result.drawable as? BitmapDrawable)?.bitmap) {
                if (expiresAt.get() > now()) {
                    memoryCache[key] = MemoryCache.Value(cached.bitmap, cached.extras + (EXPIRES_AT to expiresAt.get()))
                } else {
                    memoryCache.remove(key)
                }
            }
        }
        return result
    }

    fun read(key: String, options: Options): SourceResult? {
        val snapshot = diskCache.openSnapshot(key) ?: return null
        try {
            val (expiresAt, mimeType) = diskCache.fileSystem.read(snapshot.metadata) { readLong() to readUtf8() }
            if (expiresAt > now()) {
                options.parameters.value<AtomicLong>(EXPIRES_AT)?.set(expiresAt)
                return SourceResult(
                    source = ImageSource(snapshot.data, diskCache.fileSystem, key, snapshot),
                    mimeType = mimeType,
                    dataSource = DataSource.DISK,
                )
            }
        } catch (error: Exception) {
            snapshot.close()
            throw error
        }
        snapshot.close()
        diskCache.remove(key)
        return null
    }

    fun write(key: String, bytes: ByteArray, mimeType: String, options: Options) {
        val editor = diskCache.openEditor(key) ?: return
        try {
            val expiresAt = now() + 1.hours.inWholeMilliseconds
            diskCache.fileSystem.write(editor.metadata) {
                writeLong(expiresAt)
                writeUtf8(mimeType)
            }
            diskCache.fileSystem.write(editor.data) { write(bytes) }
            editor.commit()
            options.parameters.value<AtomicLong>(EXPIRES_AT)?.set(expiresAt)
        } catch (error: Exception) {
            editor.abort()
            throw error
        }
    }

    private companion object {
        const val EXPIRES_AT = "public-profile:expires-at"
    }
}
