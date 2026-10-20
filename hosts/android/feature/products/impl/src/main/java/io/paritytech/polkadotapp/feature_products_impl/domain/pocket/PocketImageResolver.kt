package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import javax.inject.Inject

/** Turns an image source in a face into something the image loader can fetch. */
interface PocketImageResolver {
    suspend fun resolve(productId: ProductId, source: JsImageSource): Result<Uri>
}

class RealPocketImageResolver @Inject constructor(
    private val archive: ProductWorkerArchive,
    private val ipfsContentLookup: IpfsContentLookup,
    private val pinnedPocketCards: PinnedPocketCards,
) : PocketImageResolver {
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
