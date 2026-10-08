package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.content.SharedPreferences
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.test_shared.whenever
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.mockito.Mockito.mock

class PrefsDebugPocketCardsTest {
    private val prefs: SharedPreferences = mock()
    private val productId = ProductId.fromStoredValue("game.dot")

    private fun cards(isDebugBuild: Boolean = true) = PrefsDebugPocketCards(prefs, isDebugBuild)

    private fun stored(cardId: String?, title: String? = null, previewUrl: String? = DEV_FACE) {
        whenever(prefs.getString("game.dot.card_id", null)).thenReturn(cardId)
        whenever(prefs.getString("game.dot.title", null)).thenReturn(title)
        whenever(prefs.getString("game.dot.preview_url", null)).thenReturn(previewUrl)
    }

    @Test
    fun `a spec becomes a card whose preview is the url it names`() {
        stored(cardId = "loyalty", title = "Loyalty")

        val definition = cards().get(productId)?.toDefinition()

        assertEquals(PocketCardPreview.Url(DEV_FACE), definition?.preview)
        assertEquals("Loyalty", definition?.title)
    }

    /**
     * The core screens a card id before any Pocket call. Screening here too means a typo reads as no
     * card, rather than producing a worker whose one card is refused at the first wire call — which
     * would look like the card silently not existing.
     */
    @Test
    fun `an id the core would refuse reads as no card at all`() {
        stored(cardId = "loyalty‮")

        assertNull(cards().get(productId))
    }

    @Test
    fun `an id is normalised the way the manifest path normalises one`() {
        stored(cardId = "  loyalty  ")

        assertEquals("loyalty", cards().get(productId)?.cardId?.value)
    }

    /** Without a face to draw, a card would reach the add sheet and fail there. */
    @Test
    fun `a spec with no preview url is not a card`() {
        stored(cardId = "loyalty", previewUrl = null)

        assertNull(cards().get(productId))
    }

    @Test
    fun `the title falls back to the card id rather than being blank`() {
        stored(cardId = "loyalty", title = "   ")

        assertEquals("loyalty", cards().get(productId)?.title)
    }

    // The core trims every Unicode space, NEXT LINE included, which Kotlin does not count as blank.
    // Judged before screening, this title would reach the card as an empty one.
    @Test
    fun `a title the core trims to nothing also falls back to the card id`() {
        stored(cardId = "loyalty", title = "\u0085")

        assertEquals("loyalty", cards().get(productId)?.title)
    }

    @Test
    fun `a title the core would refuse to draw reads as no card at all`() {
        stored(cardId = "loyalty", title = "Loyalty\u202E")

        assertNull(cards().get(productId))
    }

    /** A release build has no debug menu, so it must not carry whatever a debug build left behind. */
    @Test
    fun `nothing is read on a release build`() {
        stored(cardId = "loyalty", title = "Loyalty")

        assertNull(cards(isDebugBuild = false).get(productId))
    }

    private companion object {
        const val DEV_FACE = "http://127.0.0.1:5173/faces/loyalty.json"
    }
}
