package io.paritytech.polkadotapp.feature_products_impl.domain.pocketFacePreview

import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RemoteFaceSource
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import okhttp3.HttpUrl.Companion.toHttpUrl
import javax.inject.Inject

/**
 * Draws a face served from a developer's machine, through the same decoder and the same renderer the
 * Pocket tab uses. Anything else would let the preview agree with a card the app would not draw.
 */
class PocketFacePreviewInteractor @Inject constructor(
    private val remoteFaces: RemoteFaceSource,
    private val ipfsContentLookup: IpfsContentLookup,
) {
    // The face is edited between draws, so the app's HTTP cache must not answer for the server.
    suspend fun loadFace(url: String): Result<JsWidget> =
        remoteFaces.fetch(url.trim(), bypassCache = true).map { it.toJsWidget() }

    // The developer serves the folder that becomes the worker archive, so archive paths resolve on the face's server.
    suspend fun resolveImage(faceUrl: String, source: JsImageSource): Result<String> = when (source) {
        is JsImageSource.Archive -> runCatching { archiveImageUrl(faceUrl, source.path) }
        is JsImageSource.Bulletin -> ipfsContentLookup.getIpfsLinkFor(source.cid)
    }

    // Each segment is a literal file name, as the archive reads it, so the address cannot leave the face's server.
    private fun archiveImageUrl(faceUrl: String, path: String): String {
        val segments = path.split("/")
        require(!segments.climbsAboveRoot()) { "'$path' escapes the archive" }

        return segments
            .fold(faceUrl.toHttpUrl().newBuilder().encodedPath("/").query(null).fragment(null)) { url, segment ->
                url.addPathSegment(segment)
            }
            .build()
            .toString()
    }

    private fun List<String>.climbsAboveRoot(): Boolean = runningFold(0) { depth, segment ->
        when (segment) {
            ".." -> depth - 1
            "", "." -> depth
            else -> depth + 1
        }
    }.any { it < 0 }
}
