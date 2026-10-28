package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.content.Context
import android.content.res.AssetManager
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.test_shared.whenever
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.ArgumentMatchers.anyInt
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import uniffi.truapi.RendererNode
import java.io.File

class AssetPinnedPocketCardsTest {
    private val tld = requireNotNull(DotNsTld.parse("testnet"))
    private val context: Context = mock()
    private val assets: AssetManager = mock()

    // The real bundled file, not a stand-in: a face that stopped decoding against the renderer
    // vocabulary would otherwise be found by the user at their first launch, never by a test.
    private val bundledHumanityFace = File("src/main/assets/pocket/humanity.json")

    private val humanityKey = PocketCardKey(ReservedProductIds.personhood(tld), PocketCardId("humanity"))

    private fun pinnedCards(): AssetPinnedPocketCards {
        whenever(context.assets).thenReturn(assets)
        whenever(assets.open("pocket/humanity.json")).thenReturn(bundledHumanityFace.inputStream())
        whenever(context.getString(anyInt())).thenReturn("Humanity")

        return AssetPinnedPocketCards(context)
    }

    // Balance and Scarcity were dropped: the products behind them publish no Pocket cards, so pinning
    // them left two placeholder cards that duplicated the host's own native ones.
    @Test
    fun `the host pins Humanity alone, on the personhood product`() {
        assertEquals(listOf(humanityKey), pinnedCards().cards(tld).map { it.key })
    }

    @Test
    fun `a host-placed card is privileged, so nothing can remove it`() {
        assertTrue(pinnedCards().cards(tld).single().privileged)
    }

    @Test
    fun `the face bundled for Humanity decodes against the vocabulary the app ships`() {
        assertTrue("missing $bundledHumanityFace", bundledHumanityFace.exists())

        assertTrue(pinnedCards().pinned(humanityKey)?.face is RendererNode.Box)
    }

    private fun bundledImages(): AssetPinnedPocketCards {
        whenever(context.assets).thenReturn(assets)
        whenever(assets.list("pocket")).thenReturn(File("src/main/assets/pocket").list())

        return AssetPinnedPocketCards(context)
    }

    // A missing file is a hole in the card at first launch, which nothing else would notice.
    @Test
    fun `every image the bundled Humanity face draws ships with the app`() {
        val paths = Regex(""""tag"\s*:\s*"Archive"\s*,\s*"value"\s*:\s*"([^"]+)"""")
            .findAll(bundledHumanityFace.readText())
            .map { it.groupValues[1] }
            .toList()
        val cards = bundledImages()

        assertTrue(paths.isNotEmpty())
        paths.forEach { assertTrue("not shipped: $it", cards.bundlesImage(ReservedProductIds.personhood(tld), it)) }
    }

    @Test
    fun `the app's images stay on the card's own product and its own folder`() {
        val cards = bundledImages()
        val personhood = ReservedProductIds.personhood(tld)

        assertFalse(cards.bundlesImage(ProductId.fromStoredValue("someone.testnet"), "pocket/card-mark.webp"))
        listOf("pocket/wordmark-light.png", "pocket/../pocket/card-mark.webp", "card-mark.webp").forEach {
            assertFalse(it, cards.bundlesImage(personhood, it))
        }
    }

    // Both apps ship the same placeholder, so updating one copy and not the other draws a different card on each.
    @Test
    fun `the iOS app ships the same placeholder face and images`() {
        val iosFaces = File("../../../../ios/polkadot-app/Modules/Products/Pocket/Faces")

        File("src/main/assets/pocket").listFiles().orEmpty().forEach { android ->
            val ios = File(iosFaces, "pocket-${android.name}")
            assertTrue("$ios differs from ${android.name}", ios.isFile && ios.readBytes().contentEquals(android.readBytes()))
        }
    }

    // Listing the app's assets reads the APK, on whichever thread resolves the first image.
    @Test
    fun `another product's image does not list the app's assets`() {
        bundledImages().bundlesImage(ProductId.fromStoredValue("someone.testnet"), "pocket/card-mark.webp")

        verify(assets, never()).list("pocket")
    }

    // The key already says which network its product is on, so its bundled face is found offline.
    @Test
    fun `a card is recognised as pinned from its key alone`() {
        val cards = pinnedCards()

        assertEquals(humanityKey, cards.pinned(humanityKey)?.card?.key)
        assertNull(cards.pinned(PocketCardKey(ProductId.fromStoredValue("game.testnet"), PocketCardId("humanity"))))
        assertNull(cards.pinned(PocketCardKey(ReservedProductIds.personhood(tld), PocketCardId("loyalty"))))
    }
}
