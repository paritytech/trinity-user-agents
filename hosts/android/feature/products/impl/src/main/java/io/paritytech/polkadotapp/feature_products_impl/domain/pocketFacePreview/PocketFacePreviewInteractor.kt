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
    suspend fun loadFace(url: String): Result<JsWidget> = remoteFaces.fetch(url.trim()).map { it.toJsWidget() }

    // The developer serves the folder that becomes the worker archive, so archive paths resolve on the face's server.
    suspend fun resolveImage(faceUrl: String, source: JsImageSource): Result<String> = when (source) {
        is JsImageSource.Archive -> runCatching { archiveImageUrl(faceUrl, source.path) }
        is JsImageSource.Bulletin -> ipfsContentLookup.getIpfsLinkFor(source.cid)
    }

    private fun archiveImageUrl(faceUrl: String, path: String): String {
        // Checked before resolving, which quietly drops a `..` above the root.
        require(".." !in path.split("/")) { "'$path' escapes the archive" }

        val face = faceUrl.toHttpUrl()
        val image = requireNotNull(face.resolve("/")?.resolve(path)) { "'$path' is not an archive path" }

        require(image.scheme == face.scheme && image.host == face.host && image.port == face.port) {
            "'$path' leaves the face's server"
        }

        return image.toString()
    }
}
