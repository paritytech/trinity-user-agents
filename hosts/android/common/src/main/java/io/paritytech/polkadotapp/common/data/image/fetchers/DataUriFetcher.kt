package io.paritytech.polkadotapp.common.data.image.fetchers

import android.net.Uri
import coil.ImageLoader
import coil.decode.DataSource
import coil.decode.ImageSource
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.fetch.SourceResult
import coil.request.Options
import okio.Buffer
import kotlin.io.encoding.Base64

private const val DATA_SCHEME = "data"
private const val BASE64_MARKER = ";base64,"

/** Decodes a `data:<mime>;base64,<bytes>` URI, which Coil 2 has no fetcher for. */
class DataUriFetcher(
    private val dataUri: String,
    private val options: Options,
) : Fetcher {
    override suspend fun fetch(): FetchResult {
        val bytes = Base64.decode(dataUri.substringAfter(BASE64_MARKER))

        return SourceResult(
            source = ImageSource(Buffer().write(bytes), options.context),
            mimeType = dataUri.substringBefore(BASE64_MARKER).removePrefix("$DATA_SCHEME:").ifEmpty { null },
            dataSource = DataSource.MEMORY,
        )
    }

    class Factory : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher? {
            val dataUri = data.toString()
            if (!dataUri.isInlineImageUri()) return null

            return DataUriFetcher(dataUri, options)
        }
    }
}

/** Whether [this] is an image inline in a base64 `data:` URI, drawable without a network fetch. */
fun String.isInlineImageUri(): Boolean = startsWith("$DATA_SCHEME:") && BASE64_MARKER in this
