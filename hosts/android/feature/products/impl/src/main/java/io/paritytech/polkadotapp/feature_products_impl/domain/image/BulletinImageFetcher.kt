package io.paritytech.polkadotapp.feature_products_impl.domain.image

import coil.ComponentRegistry
import coil.ImageLoader
import coil.annotation.ExperimentalCoilApi
import coil.decode.DataSource
import coil.decode.ImageSource
import coil.disk.DiskCache
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.fetch.SourceResult
import coil.key.Keyer
import coil.request.Options
import io.paritytech.polkadotapp.common.data.image.ImageLoaderComponentRegistrar
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import okio.Buffer
import uniffi.truapi.ImageFormat
import uniffi.truapi.ImageReference
import javax.inject.Inject

@OptIn(ExperimentalCoilApi::class)
internal class BulletinImageFetcher(
    private val reference: ImageReference.Bulletin,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val options: Options,
    private val diskCache: DiskCache?,
) : Fetcher {
    override suspend fun fetch(): FetchResult? {
        if (options.diskCachePolicy.readEnabled) {
            runCancellableCatching { readCached() }
                .logFailure("Cached Bulletin image ${reference.cid}")
                .getOrElse { return null }
                ?.let { return it }
        }

        val image = runtimeProvider.runtime()
            .flatMap { runtime -> runCancellableCatching { runtime.loadImage(reference) } }
            .logFailure("Bulletin image ${reference.cid}")
            .getOrNull()
            ?: return null

        if (options.diskCachePolicy.writeEnabled) {
            runCancellableCatching { writeCached(image.bytes, image.format) }
                .logFailure("Caching Bulletin image ${reference.cid}")
        }

        return SourceResult(
            source = ImageSource(Buffer().write(image.bytes), options.context),
            mimeType = image.format.mimeType(),
            dataSource = DataSource.NETWORK,
        )
    }

    private fun readCached(): SourceResult? {
        val cache = diskCache ?: return null
        val snapshot = cache.openSnapshot(reference.cid) ?: return null
        try {
            val format = cache.fileSystem.read(snapshot.metadata) { ImageFormat.valueOf(readUtf8()) }
            check(reference.declaredFormat == null || format == reference.declaredFormat) { "Cached image does not match declared format" }
            return SourceResult(
                source = ImageSource(snapshot.data, cache.fileSystem, reference.cid, snapshot),
                mimeType = format.mimeType(),
                dataSource = DataSource.DISK,
            )
        } catch (error: Exception) {
            snapshot.close()
            throw error
        }
    }

    private fun writeCached(bytes: ByteArray, format: ImageFormat) {
        val cache = diskCache ?: return
        val editor = cache.openEditor(reference.cid) ?: return
        try {
            cache.fileSystem.write(editor.metadata) { writeUtf8(format.name) }
            cache.fileSystem.write(editor.data) { write(bytes) }
            editor.commit()
        } catch (error: Exception) {
            editor.abort()
            throw error
        }
    }

    private fun ImageFormat.mimeType(): String = when (this) {
        ImageFormat.PNG -> "image/png"
        ImageFormat.JPEG -> "image/jpeg"
        ImageFormat.GIF -> "image/gif"
        ImageFormat.WEBP -> "image/webp"
    }

    class Factory(private val runtimeProvider: TrUAPIHostRuntimeProvider) : Fetcher.Factory<ImageReference.Bulletin> {
        override fun create(data: ImageReference.Bulletin, options: Options, imageLoader: ImageLoader): Fetcher =
            BulletinImageFetcher(data, runtimeProvider, options, imageLoader.diskCache)
    }
}

internal class BulletinImageLoaderRegistrar @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) : ImageLoaderComponentRegistrar {
    override fun register(builder: ComponentRegistry.Builder) {
        builder.add(BulletinImageKeyer())
        builder.add(BulletinImageFetcher.Factory(runtimeProvider))
    }
}

private class BulletinImageKeyer : Keyer<ImageReference.Bulletin> {
    override fun key(data: ImageReference.Bulletin, options: Options): String = "${data.cid}:${data.declaredFormat}"
}
