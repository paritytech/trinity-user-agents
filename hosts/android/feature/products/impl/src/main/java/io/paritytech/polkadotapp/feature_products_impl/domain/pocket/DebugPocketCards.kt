package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.content.SharedPreferences
import androidx.core.content.edit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardDefinition
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import uniffi.truapi.screenPocketCardId
import uniffi.truapi.screenPocketCardTitle
import java.net.URI

/** The card a debug-supplied worker declares, so a worker served from a laptop can carry one. */
data class DebugPocketCard(
    val cardId: PocketCardId,
    val title: String,
    val previewUrl: String,
    val faceShown: Boolean = true,
)

/**
 * A card spec entered in the debug menu, alongside that product's worker URL, and the loopback
 * page the product shows when it is opened.
 *
 * Kept out of the database on purpose: this is a developer's scratch setting, not app state worth a
 * schema revision.
 */
interface DebugPocketCards {
    fun get(productId: ProductId): DebugPocketCard?

    fun set(productId: ProductId, card: DebugPocketCard?)

    /** Where the product's page is served from on the developer's machine, if it is. */
    fun appUrl(productId: ProductId): String?

    fun setAppUrl(productId: ProductId, appUrl: String?)
}

class PrefsDebugPocketCards(
    private val prefs: SharedPreferences,
    private val isDebugBuild: Boolean,
) : DebugPocketCards {
    override fun get(productId: ProductId): DebugPocketCard? {
        if (!isDebugBuild) return null

        val rawId = prefs.getString(productId.key(CARD_ID), null) ?: return null
        val previewUrl = prefs.getString(productId.key(PREVIEW_URL), null)?.takeIf { it.isNotBlank() } ?: return null

        val rawTitle = prefs.getString(productId.key(TITLE), null)

        // Screened with the rules the manifest path uses, so a debug card cannot carry an id or a
        // title the core would refuse. A rejected one reads as no card at all.
        return runCatching {
            val cardId = screenPocketCardId(rawId)
            DebugPocketCard(
                cardId = PocketCardId(cardId),
                title = rawTitle?.let(::screenPocketCardTitle)?.takeIf { it.isNotEmpty() } ?: cardId,
                previewUrl = previewUrl,
                faceShown = prefs.getBoolean(productId.key(FACE_SHOWN), true),
            )
        }
            .logFailure("pocket: debug card for $productId is not usable")
            .getOrNull()
    }

    override fun set(productId: ProductId, card: DebugPocketCard?) {
        prefs.edit {
            putString(productId.key(CARD_ID), card?.cardId?.value)
            putString(productId.key(TITLE), card?.title)
            putString(productId.key(PREVIEW_URL), card?.previewUrl)
            putBoolean(productId.key(FACE_SHOWN), card?.faceShown ?: true)
        }
    }

    override fun appUrl(productId: ProductId): String? {
        if (!isDebugBuild) return null

        return prefs.getString(productId.key(APP_URL), null)?.takeIf(::isLoopbackUrl)
    }

    override fun setAppUrl(productId: ProductId, appUrl: String?) {
        prefs.edit { putString(productId.key(APP_URL), appUrl) }
    }

    // The network security config allows cleartext to 127.0.0.1 only, so any other host would fail to load.
    private fun isLoopbackUrl(url: String): Boolean =
        runCatching { URI(url) }.getOrNull()?.let { it.scheme == "http" && it.host == LOOPBACK_HOST } == true

    private fun ProductId.key(field: String) = "$value.$field"

    private companion object {
        const val CARD_ID = "card_id"
        const val TITLE = "title"
        const val PREVIEW_URL = "preview_url"
        const val FACE_SHOWN = "face_shown"
        const val APP_URL = "app_url"
        const val LOOPBACK_HOST = "127.0.0.1"
    }
}

fun DebugPocketCard.toDefinition(): PocketCardDefinition = PocketCardDefinition(
    id = cardId,
    title = title,
    preview = PocketCardPreview.Url(previewUrl),
    faceShown = faceShown,
)
