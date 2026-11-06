package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import android.net.Uri
import coil.ComponentRegistry
import coil.ImageLoader
import coil.decode.DataSource
import coil.decode.ImageSource
import coil.disk.DiskCache
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.fetch.SourceResult
import coil.memory.MemoryCache
import coil.request.Options
import io.paritytech.polkadotapp.common.data.image.ImageLoaderComponentRegistrar
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.PUBLIC_PROFILE_AVATAR_SCHEME
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.publicProfileAvatarUri
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import okio.Buffer
import uniffi.truapi.ImageFormat
import uniffi.truapi.ImageReference
import javax.inject.Inject

/**
 * Loads a [publicProfileAvatarUri][io.paritytech.polkadotapp.feature_chats_api.domain.profile.publicProfileAvatarUri]
 * through the core. No avatar, or a failed lookup, leaves the fetch unanswered and the placeholder drawn. A request
 * that skips the disk cache skips the core's cached profile record too.
 */
internal class PublicProfileAvatarFetcher(
    private val username: String,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val options: Options,
    private val cache: PublicProfileAvatarCache,
) : Fetcher {
    override suspend fun fetch(): FetchResult? {
        val key = options.diskCacheKey ?: publicProfileAvatarUri(username)
        if (options.diskCachePolicy.readEnabled) {
            runCancellableCatching { cache.read(key, options) }
                .logFailure("Cached public profile avatar of $username")
                .getOrNull()
                ?.let { return it }
        }

        val avatar = runtimeProvider.runtime()
            .flatMap { runtime ->
                runCancellableCatching { runtime.loadImage(ImageReference.Username(username, force = !options.diskCachePolicy.readEnabled)) }
            }
            .logFailure("Public profile avatar of $username")
            .getOrNull()
            ?: return null

        if (options.diskCachePolicy.writeEnabled) {
            runCancellableCatching { cache.write(key, avatar.bytes, avatar.format.mimeType(), options) }
                .logFailure("Caching public profile avatar of $username")
        }

        return SourceResult(
            source = ImageSource(Buffer().write(avatar.bytes), options.context),
            mimeType = avatar.format.mimeType(),
            dataSource = DataSource.NETWORK,
        )
    }

    private fun ImageFormat.mimeType(): String = when (this) {
        ImageFormat.PNG -> "image/png"
        ImageFormat.JPEG -> "image/jpeg"
        ImageFormat.GIF -> "image/gif"
        ImageFormat.WEBP -> "image/webp"
    }

    class Factory(
        private val runtimeProvider: TrUAPIHostRuntimeProvider,
        private val cache: PublicProfileAvatarCache,
    ) : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher? {
            if (data.scheme != PUBLIC_PROFILE_AVATAR_SCHEME) return null

            return PublicProfileAvatarFetcher(data.schemeSpecificPart, runtimeProvider, options, cache)
        }
    }
}

internal class PublicProfileAvatarImageLoaderRegistrar @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    diskCache: DiskCache,
    memoryCache: MemoryCache,
) : ImageLoaderComponentRegistrar {
    private val cache = PublicProfileAvatarCache(diskCache, memoryCache)

    override fun register(builder: ComponentRegistry.Builder) {
        builder.add(cache)
        builder.add(PublicProfileAvatarFetcher.Factory(runtimeProvider, cache))
    }
}
