package io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.readNBytesCompat
import io.paritytech.polkadotapp.feature_dotns_impl.di.DotNsHttpClient
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import io.paritytech.polkadotapp.tools_ipfs_api.Cids
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import io.paritytech.polkadotapp.tools_ipfs_api.getIpfsLinkFor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import timber.log.Timber
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import javax.inject.Inject

/**
 * Fetches CAR archives from IPFS gateway.
 *
 * Handles two deployment formats for backward compatibility:
 * - **Legacy**: The CID points to an uploaded CAR file. Fetching raw returns the CAR directly.
 * - **New**: The CID points to a directory. Must fetch with `?format=car` to get all files.
 *
 * The fetcher first tries a raw fetch. If the response is already a CAR file, it's used directly.
 * Otherwise, it re-fetches with `?format=car` to get the directory as a CAR archive.
 */
class RealCarFetcher @Inject constructor(
    @param:ApplicationContext private val context: Context,
    @param:DotNsHttpClient private val httpClient: OkHttpClient,
    private val ipfsContentLookup: IpfsContentLookup
) : CarFetcher {
    override suspend fun fetchCar(contentHash: ByteArray): Result<ByteArray> {
        return withContext(Dispatchers.IO) {
            Cids.castCatching(contentHash)
                .flatMap { cid -> ipfsContentLookup.getIpfsLinkFor(cid) }
                .mapCatching { baseUrl ->
                    // Phase 1: Fetch raw (no format override)
                    val rawBytes = fetch(baseUrl)

                    if (CarParser.looksLikeCarArchive(rawBytes)) {
                        // Legacy: CID is an uploaded CAR file — use directly
                        rawBytes
                    } else {
                        // New: CID is a directory — re-fetch with ?format=car
                        fetch("$baseUrl?format=car", accept = "application/vnd.ipld.car")
                    }
                }
        }
    }

    override suspend fun fetchCarToFile(
        contentHash: ByteArray,
        onProgress: (downloaded: Long, total: Long?) -> Unit
    ): Result<File> {
        return withContext(Dispatchers.IO) {
            Cids.castCatching(contentHash)
                .flatMap { cid -> ipfsContentLookup.getIpfsLinkFor(cid) }
                .mapCatching { baseUrl ->
                    // Phase 1: raw fetch — a legacy CID points directly at an uploaded CAR file.
                    val rawFile = fetchToFile(baseUrl, onProgress = onProgress)

                    if (looksLikeCarArchive(rawFile)) {
                        rawFile
                    } else {
                        // New format: the CID is a directory — re-fetch it exported as a CAR archive.
                        rawFile.delete()
                        fetchToFile("$baseUrl?format=car", accept = CAR_MEDIA_TYPE, onProgress = onProgress)
                    }
                }
        }
    }

    private fun fetch(url: String, accept: String? = null): ByteArray {
        val requestBuilder = Request.Builder().url(url)
        if (accept != null) {
            requestBuilder.header("Accept", accept)
        }

        return httpClient.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("IPFS fetch failed: HTTP ${response.code} for $url")
            }
            response.body.bytes()
        }
    }

    private fun fetchToFile(
        url: String,
        accept: String? = null,
        onProgress: (downloaded: Long, total: Long?) -> Unit
    ): File {
        val requestBuilder = Request.Builder().url(url)
        if (accept != null) {
            requestBuilder.header("Accept", accept)
        }

        return httpClient.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("IPFS fetch failed: HTTP ${response.code} for $url")
            }

            val total = response.body.contentLength()
            val tempFile = File.createTempFile(TEMP_CAR_PREFIX, TEMP_CAR_SUFFIX, context.cacheDir)
            try {
                response.body.byteStream().use { input ->
                    tempFile.outputStream().use { output -> input.copyReporting(output, total, onProgress) }
                }
                tempFile
            } catch (e: Throwable) {
                tempFile.delete()
                throw e
            }
        }
    }

    /** Copies the stream while reporting cumulative bytes, throttled to avoid per-chunk churn. */
    private inline fun InputStream.copyReporting(
        output: OutputStream,
        total: Long,
        onProgress: (downloaded: Long, total: Long) -> Unit
    ) {
        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
        var downloaded = 0L
        var lastReported = 0L

        while (true) {
            val read = read(buffer)
            if (read < 0) break
            output.write(buffer, 0, read)
            downloaded += read
            if (downloaded - lastReported >= PROGRESS_REPORT_STEP_BYTES) {
                Timber.d("On Progress: $downloaded / $total")
                onProgress(downloaded, total)
                lastReported = downloaded
            }
        }

        onProgress(downloaded, total)
    }

    private fun looksLikeCarArchive(file: File): Boolean {
        val prefix = file.inputStream().use { it.readNBytesCompat(CAR_HEADER_PROBE_BYTES) }
        return CarParser.looksLikeCarArchive(prefix)
    }

    private companion object {
        const val TEMP_CAR_PREFIX = "dotns_car"
        const val TEMP_CAR_SUFFIX = ".car"
        const val CAR_MEDIA_TYPE = "application/vnd.ipld.car"

        // The CARv1 header (varint length + CBOR roots) is well under 1 KB; this prefix is a safe over-read.
        const val CAR_HEADER_PROBE_BYTES = 64 * 1024

        // Report download progress at most once per this many bytes, to avoid per-chunk StateFlow churn.
        const val PROGRESS_REPORT_STEP_BYTES = 256 * 1024L
    }
}
