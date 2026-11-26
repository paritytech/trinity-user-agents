package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import uniffi.truapi.PinnedPocketCard
import uniffi.truapi.ProductId
import uniffi.truapi.RendererNode
import uniffi.truapi.pinnedPocketCards
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

/** The cards the core pins: present on first run, removable by nobody. */
interface PinnedPocketCards {
    /** The host-placed card [key] names, if it names one. A card's own key already says which network it belongs to. */
    fun pinned(key: PocketCardKey): CachedPocketCard?

    /** Whether the app ships the image at [path] for the product behind a pinned card, so it needs no archive. */
    fun bundlesImage(productId: ProductId, path: String): Boolean
}

/**
 * The cards the core pins, each drawn from a face bundled with the app until its product streams
 * its own.
 */
@Singleton
class AssetPinnedPocketCards @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : PinnedPocketCards {
    private val bundledFaces = ConcurrentHashMap<String, RendererNode>()
    private val bundledImages by lazy { context.assets.list("pocket").orEmpty().map { "pocket/$it" }.toSet() }

    override fun pinned(key: PocketCardKey): CachedPocketCard? {
        val card = pinnedOn(key.productId).firstOrNull { it.cardId == key.cardId.value } ?: return null

        return CachedPocketCard(
            card = PocketCard(key, card.title, privileged = true),
            face = bundledFace(card.cardId),
        )
    }

    override fun bundlesImage(productId: ProductId, path: String): Boolean =
        pinnedOn(productId).isNotEmpty() && path in bundledImages

    private fun pinnedOn(productId: ProductId): List<PinnedPocketCard> {
        val tld = DotNsTld.parse(productId.value.substringAfter('.', missingDelimiterValue = "")) ?: return emptyList()

        return pinnedPocketCards(tld.value).filter { it.productId == productId }
    }

    // Bundled with the app, so a failure here is a build defect rather than product input.
    private fun bundledFace(cardId: String): RendererNode = bundledFaces.getOrPut(cardId) {
        readPocketFace(context.assets.open("pocket/$cardId.json").bufferedReader().readText())
            .getOrElse { throw IllegalStateException("bundled Pocket face '$cardId' is invalid", it) }
    }
}
