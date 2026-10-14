package io.paritytech.polkadotapp.tools_ipfs_impl.data

import android.content.Context
import android.webkit.MimeTypeMap
import coil.ImageLoader
import coil.decode.DataSource
import coil.decode.ImageSource
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.fetch.SourceResult
import coil.request.Options
import com.google.gson.Gson
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.util.fromJson
import io.paritytech.polkadotapp.common.utils.await
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsFileMetadata
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsImageRequest
import io.paritytech.polkadotapp.tools_ipfs_api.getDefaultJsonLink
import io.paritytech.polkadotapp.tools_ipfs_api.getDefaultRawLink
import okhttp3.OkHttpClient
import okhttp3.Request
import okio.Buffer

internal class IpfsImageFetcher(
    private val context: Context,
    private val okHttpClient: OkHttpClient,
    private val gson: Gson,
    private val ipfsContentLookup: IpfsContentLookup,
    private val request: IpfsImageRequest,
) : Fetcher {
    override suspend fun fetch(): FetchResult {
        val url = ipfsContentLookup.getDefaultJsonLink(request.contentHash)
            .mapError { IllegalStateException("Failed to resolve IPFS metadata link", it) }
            .getOrThrow()

        val metadataResponse = fetchMetadata(url)

        val buffer = Buffer()
        metadataResponse.chunks.forEach { chunkHash ->
            val chunkUrl = ipfsContentLookup.getDefaultRawLink(chunkHash.fromHex())
                .mapError { IllegalStateException("Failed to resolve IPFS chunk link", it) }
                .getOrThrow()

            val request = Request.Builder().url(chunkUrl).build()
            val response = okHttpClient.newCall(request).await()

            if (!response.isSuccessful) {
                throw IllegalStateException("Failed to fetch image chunk")
            }

            val source = response.body?.source()?.buffer ?: throw IllegalStateException("Null response body")

            buffer.writeAll(source)
        }

        val extension = metadataResponse.path.substringAfterLast('.', "")
        val mimeType = MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)

        return SourceResult(
            source = ImageSource(buffer, context),
            mimeType = mimeType,
            dataSource = DataSource.NETWORK
        )
    }

    private suspend fun fetchMetadata(metadataUrl: String): IpfsFileMetadata {
        val request = Request.Builder().url(metadataUrl).build()
        val response = okHttpClient.newCall(request).await()

        if (!response.isSuccessful) {
            throw IllegalStateException("Failed to fetch metadata")
        }

        val responseBody = response.body?.string() ?: throw IllegalStateException("Null response body")

        return gson.fromJson(responseBody)
    }

    class Factory(
        private val okHttpClient: OkHttpClient,
        private val gson: Gson,
        private val ipfsContentLookup: IpfsContentLookup,
    ) : Fetcher.Factory<IpfsImageRequest> {
        override fun create(data: IpfsImageRequest, options: Options, imageLoader: ImageLoader): Fetcher {
            return IpfsImageFetcher(options.context, okHttpClient, gson, ipfsContentLookup, data)
        }
    }
}
