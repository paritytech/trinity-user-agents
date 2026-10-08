package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.readNBytesCompat
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Request
import uniffi.truapi.RendererNode
import java.io.InputStream
import javax.inject.Inject

/** The most of a face the host reads, wherever it is served from. */
internal const val MAX_FACE_BYTES = 256L * 1024

/**
 * Reads one byte past the bound, so a source that declares no length, or lies about it, is refused
 * rather than read to exhaustion.
 */
internal fun InputStream.readFaceWithinBound(): String {
    val bytes = readNBytesCompat((MAX_FACE_BYTES + 1).toInt())
    require(bytes.size <= MAX_FACE_BYTES) { "face is larger than $MAX_FACE_BYTES bytes" }

    return bytes.decodeToString()
}

/** A face tree served over HTTP, for faces that do not live in a product's worker archive. */
interface RemoteFaceSource {
    suspend fun fetch(url: String): Result<RendererNode>
}

/**
 * Nothing here is reachable from a published manifest, but a dev server is still a remote the host
 * does not control, so the same bound applies as to a face inside an archive.
 */
class OkHttpRemoteFaceSource @Inject constructor(
    private val calls: Call.Factory,
    private val dispatchers: CoroutineDispatchers,
) : RemoteFaceSource {
    // `execute` blocks, and both callers collect on the main thread. Left there, Android answers with
    // a `NetworkOnMainThreadException`, which carries no message and so tells the screen nothing.
    override suspend fun fetch(url: String): Result<RendererNode> = withContext(dispatchers.io) {
        runCatching {
            val request = Request.Builder().url(url).build()

            calls.newCall(request).execute().use { response ->
                require(response.isSuccessful) { "face at $url answered ${response.code}" }
                val body = requireNotNull(response.body) { "face at $url has no body" }

                body.byteStream().readFaceWithinBound()
            }
        }
            .fold(onSuccess = ::readPocketFace, onFailure = Result.Companion::failure)
    }
}
