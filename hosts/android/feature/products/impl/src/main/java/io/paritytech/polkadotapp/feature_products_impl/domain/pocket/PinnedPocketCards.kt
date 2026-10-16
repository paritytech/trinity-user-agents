package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.content.Context
import androidx.annotation.StringRes
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.truapi.RendererNode
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

/** The cards the host itself places: present on first run, removable by nobody. */
interface PinnedPocketCards {
    suspend fun cards(): List<CachedPocketCard>

    /**
     * The host-placed card [key] names, if it names one. Answers without waiting on anything: a
     * removal arrives from the core on a thread it cannot spare, and a card's own key already says
     * which network it belongs to.
     */
    fun pinned(key: PocketCardKey): CachedPocketCard?
}

/**
 * Humanity, backed by the governance-reserved personhood product and drawn from a face bundled with
 * the app until that product streams its own.
 */
@Singleton
class AssetPinnedPocketCards @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val dotNsTldProvider: DotNsTldProvider,
) : PinnedPocketCards {
    private class Definition(
        val cardId: String,
        @StringRes val titleRes: Int,
        val backingProduct: (DotNsTld) -> ProductId,
    )

    private val definitions = listOf(
        Definition("humanity", RCommon.string.pocket_pinned_card_humanity, ReservedProductIds::personhood),
    )

    private val loading = Mutex()
    private var loaded: List<CachedPocketCard>? = null
    private val bundledFaces = ConcurrentHashMap<String, RendererNode>()

    // The card's product is reserved on the network the app is on, which can take a chain read to
    // learn. Only listing the cards needs it; naming one does not.
    override suspend fun cards(): List<CachedPocketCard> = loading.withLock {
        loaded ?: load().also { loaded = it }
    }

    override fun pinned(key: PocketCardKey): CachedPocketCard? {
        val definition = definitions.firstOrNull { it.cardId == key.cardId.value } ?: return null
        val tld = DotNsTld.parse(key.productId.value.substringAfter('.', missingDelimiterValue = "")) ?: return null
        if (definition.backingProduct(tld) != key.productId) return null

        return definition.toCard(key.productId)
    }

    private suspend fun load(): List<CachedPocketCard> {
        val tld = dotNsTldProvider.getTldRetrying()

        return definitions.map { it.toCard(it.backingProduct(tld)) }
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
