package io.paritytech.polkadotapp.feature_products_impl.domain.product

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PinnedPocketCards
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import javax.inject.Inject

/** Turns an image source in a Pocket face or a chat card into something the image loader can fetch. */
interface ProductImageResolver {
    suspend fun resolve(productId: ProductId, source: JsImageSource): Result<Uri>
}

class RealProductImageResolver @Inject constructor(
    private val archive: ProductWorkerArchive,
    private val ipfsContentLookup: IpfsContentLookup,
    private val pinnedPocketCards: PinnedPocketCards,
) : ProductImageResolver {
    override suspend fun resolve(productId: ProductId, source: JsImageSource): Result<Uri> = when (source) {
        // shortcut: the app's copy shadows the product's file at the same path until the next app release.
        is JsImageSource.Archive -> if (pinnedPocketCards.bundlesImage(productId, source.path)) {
            Result.success("file:///android_asset/${source.path}".toUri())
        } else {
            archive.file(productId, source.path).map(Uri::fromFile)
        }
        is JsImageSource.Bulletin -> ipfsContentLookup.getIpfsLinkFor(source.cid).map { it.toUri() }
    }
}
