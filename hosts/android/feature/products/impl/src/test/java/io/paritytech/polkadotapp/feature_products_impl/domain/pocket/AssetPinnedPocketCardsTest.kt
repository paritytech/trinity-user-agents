package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.content.Context
import android.content.res.AssetManager
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.ArgumentMatchers.anyInt
import org.mockito.Mockito.mock
import uniffi.truapi.RendererNode
import java.io.File

class AssetPinnedPocketCardsTest {
    private val tld = requireNotNull(DotNsTld.parse("testnet"))
    private val context: Context = mock()
    private val assets: AssetManager = mock()
    private val tldProvider: DotNsTldProvider = mock()

    // The real bundled file, not a stand-in: a face that stopped decoding against the renderer
    // vocabulary would otherwise be found by the user at their first launch, never by a test.
    private val bundledHumanityFace = File("src/main/assets/pocket/humanity.json")

    private suspend fun pinnedCards(): List<CachedPocketCard> {
        whenever(context.assets).thenReturn(assets)
        whenever(assets.open("pocket/humanity.json")).thenReturn(bundledHumanityFace.inputStream())
        whenever(context.getString(anyInt())).thenReturn("Humanity")
        whenever(tldProvider.getTld()).thenReturn(Result.success(tld))

        return AssetPinnedPocketCards(context, tldProvider).cards()
    }

    // Balance and Scarcity were dropped: the products behind them publish no Pocket cards, so pinning
    // them left two placeholder cards that duplicated the host's own native ones.
    @Test
    fun `the host pins Humanity alone, on the personhood product`() = runBlocking {
        val cards = pinnedCards()

        assertEquals(1, cards.size)
        assertEquals(ReservedProductIds.personhood(tld), cards.single().card.key.productId)
        assertEquals("humanity", cards.single().card.key.cardId.value)
    }

    @Test
    fun `a host-placed card is privileged, so nothing can remove it`() = runBlocking {
        assertTrue(pinnedCards().single().card.privileged)
    }

    @Test
    fun `the face bundled for Humanity decodes against the vocabulary the app ships`() = runBlocking {
        assertTrue("missing $bundledHumanityFace", bundledHumanityFace.exists())

        assertTrue(pinnedCards().single().face is RendererNode.Column)
    }

    // The key already says which network its product is on, so naming a pinned card needs no chain
    // read. Offline that read never returns, and a removal arriving from the core would wait on it.
    @Test
    fun `a card is recognised as pinned without asking the network for its name`() = runBlocking {
        whenever(context.assets).thenReturn(assets)
        whenever(assets.open("pocket/humanity.json")).thenReturn(bundledHumanityFace.inputStream())
        whenever(context.getString(anyInt())).thenReturn("Humanity")
        val neverAnswers: DotNsTldProvider = mock()
        val pinnedKey = PocketCardKey(ReservedProductIds.personhood(tld), PocketCardId("humanity"))

        val cards = AssetPinnedPocketCards(context, neverAnswers)

        assertEquals(pinnedKey, cards.pinned(pinnedKey)?.card?.key)
        assertNull(cards.pinned(PocketCardKey(ProductId.fromStoredValue("game.testnet"), PocketCardId("humanity"))))
        assertNull(cards.pinned(PocketCardKey(ReservedProductIds.personhood(tld), PocketCardId("loyalty"))))
    }
}
