package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardDefinition
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import uniffi.truapi.RendererNode
import java.io.File
import javax.inject.Inject

/** Reads a published card's static face, from the product's worker archive or from a dev server. */
class PocketPreviewLoader @Inject constructor(
    private val archive: ProductWorkerArchive,
    private val remoteFaces: RemoteFaceSource,
) {
    suspend fun load(productId: ProductId, definition: PocketCardDefinition): Result<RendererNode> =
        when (val preview = definition.preview) {
            is PocketCardPreview.Archive -> archive.file(productId, preview.path)
                .mapCatching { it.readWithinBound() }
                .flatMap(::readPocketFace)

            is PocketCardPreview.Url -> remoteFaces.fetch(preview.url)
        }

    // The preview is read before the user has approved anything, so how much there is to read is the
    // product's choice. Its size is checked rather than its content: by the time a hostile one has
    // been decoded it has already been held whole in memory.
    private fun File.readWithinBound(): String {
        require(length() <= MAX_FACE_BYTES) { "preview '$name' is larger than $MAX_FACE_BYTES bytes" }

        return readText()
    }
}
