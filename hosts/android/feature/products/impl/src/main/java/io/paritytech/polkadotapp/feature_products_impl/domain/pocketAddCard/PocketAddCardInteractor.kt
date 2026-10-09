package io.paritytech.polkadotapp.feature_products_impl.domain.pocketAddCard

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.CachedPocketCard
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketCardStore
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketPreviewLoader
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PublishedPocketCards
import uniffi.truapi.RendererNode
import javax.inject.Inject

/** A published card as the approval sheet shows it: the face the user approves is the one that is stored. */
data class PocketAddCardOffer(
    val key: PocketCardKey,
    val productName: String,
    val title: String,
    val face: RendererNode,
)

class PocketAddCardInteractor @Inject constructor(
    private val publishedCards: PublishedPocketCards,
    private val previewLoader: PocketPreviewLoader,
    private val store: PocketCardStore,
    private val images: PocketImageResolver,
) {
    suspend fun loadOffer(productId: ProductId, cardId: PocketCardId): Result<PocketAddCardOffer> =
        publishedCards.find(productId, cardId).flatMap { published ->
            previewLoader.load(productId, published.definition).map { face ->
                PocketAddCardOffer(
                    key = PocketCardKey(productId, cardId),
                    productName = published.product.name,
                    title = published.definition.title,
                    face = face,
                )
            }
        }

    suspend fun resolveFaceImage(productId: ProductId, source: JsImageSource): Result<Uri> =
        images.resolve(productId, source)

    suspend fun approve(offer: PocketAddCardOffer): Result<Unit> = runCatching {
        store.addCard(CachedPocketCard(PocketCard(offer.key, offer.title, privileged = false), offer.face))
    }
}
