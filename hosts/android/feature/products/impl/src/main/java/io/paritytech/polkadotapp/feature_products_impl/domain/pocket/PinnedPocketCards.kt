package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.content.Context
import androidx.annotation.StringRes
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import uniffi.truapi.PinnedPocketCard
import uniffi.truapi.RendererNode
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

/** The cards the host itself places: present on first run, removable by nobody. */
interface PinnedPocketCards {
    /** The cards placed on the network [tld] names, which the runtime is started with. */
    fun cards(tld: DotNsTld): List<PinnedPocketCard>

    /** The host-placed card [key] names, if it names one. A card's own key already says which network it belongs to. */
    fun pinned(key: PocketCardKey): CachedPocketCard?

    /** Whether the app ships the image at [path] for the product behind a pinned card, so it needs no archive. */
    fun bundlesImage(productId: ProductId, path: String): Boolean
}

/**
 * Humanity, backed by the governance-reserved personhood product and drawn from a face bundled with
 * the app until that product streams its own.
 */
@Singleton
class AssetPinnedPocketCards @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : PinnedPocketCards {
    private class Definition(
        val cardId: String,
        @StringRes val titleRes: Int,
        val backingProduct: (DotNsTld) -> ProductId,
    )

    private val definitions = listOf(
        Definition("humanity", RCommon.string.pocket_pinned_card_humanity, ReservedProductIds::personhood),
    )

    private val bundledFaces = ConcurrentHashMap<String, RendererNode>()
    private val bundledImages by lazy { context.assets.list("pocket").orEmpty().map { "pocket/$it" }.toSet() }

    override fun cards(tld: DotNsTld): List<PinnedPocketCard> = definitions.map {
        PinnedPocketCard(it.backingProduct(tld).value, it.cardId, context.getString(it.titleRes))
    }

    override fun pinned(key: PocketCardKey): CachedPocketCard? {
        val definition = definitions.firstOrNull { it.cardId == key.cardId.value } ?: return null
        if (!definition.backs(key.productId)) return null

        return definition.toCard(key.productId)
    }

    override fun bundlesImage(productId: ProductId, path: String): Boolean =
        definitions.any { it.backs(productId) } && path in bundledImages

    private fun Definition.backs(productId: ProductId): Boolean {
        val tld = DotNsTld.parse(productId.value.substringAfter('.', missingDelimiterValue = "")) ?: return false

        return backingProduct(tld) == productId
    }

    private fun Definition.toCard(backing: ProductId) = CachedPocketCard(
        card = PocketCard(
            key = PocketCardKey(backing, PocketCardId(cardId)),
            title = context.getString(titleRes),
            privileged = true,
        ),
        face = bundledFace(cardId),
    )

    // Bundled with the app, so a failure here is a build defect rather than product input.
    private fun bundledFace(cardId: String): RendererNode = bundledFaces.getOrPut(cardId) {
        readPocketFace(context.assets.open("pocket/$cardId.json").bufferedReader().readText())
            .getOrElse { throw IllegalStateException("bundled Pocket face '$cardId' is invalid", it) }
    }
}
