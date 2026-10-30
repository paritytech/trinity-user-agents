package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.content.Context
import android.content.res.AssetManager
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.RendererNode
import java.io.File

class AssetPinnedPocketCardsTest {
    private val tld = requireNotNull(DotNsTld.parse("testnet"))
    private val context: Context = mockk()
    private val assets: AssetManager = mockk()
    private val tldProvider: DotNsTldProvider = mockk()

    // The real bundled file, not a stand-in: a face that stopped decoding against the renderer
    // vocabulary would otherwise be found by the user at their first launch, never by a test.
    private val bundledHumanityFace = File("src/main/assets/pocket/humanity.json")

    private suspend fun pinnedCards(): List<CachedPocketCard> {
        every { context.assets } returns assets
        every { assets.open("pocket/humanity.json") } returns bundledHumanityFace.inputStream()
        every { context.getString(any()) } returns "Humanity"
        coEvery { tldProvider.getTld() } returns Result.success(tld)

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

        assertTrue(pinnedCards().single().face is RendererNode.Box)
    }

    private fun bundledImages(): AssetPinnedPocketCards {
        every { context.assets } returns assets
        every { assets.list("pocket") } returns File("src/main/assets/pocket").list()

        return AssetPinnedPocketCards(context, tldProvider)
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

        verify(exactly = 0) { assets.list("pocket") }
    }

    // The key already says which network its product is on, so naming a pinned card needs no chain
    // read. Offline that read never returns, and a removal arriving from the core would wait on it.
    @Test
    fun `a card is recognised as pinned without asking the network for its name`() = runBlocking {
        every { context.assets } returns assets
        every { assets.open("pocket/humanity.json") } returns bundledHumanityFace.inputStream()
        every { context.getString(any()) } returns "Humanity"
        val neverAnswers: DotNsTldProvider = mockk()
        val pinnedKey = PocketCardKey(ReservedProductIds.personhood(tld), PocketCardId("humanity"))

        val cards = AssetPinnedPocketCards(context, neverAnswers)

        assertEquals(pinnedKey, cards.pinned(pinnedKey)?.card?.key)
        assertNull(cards.pinned(PocketCardKey(ProductId.fromStoredValue("game.testnet"), PocketCardId("humanity"))))
        assertNull(cards.pinned(PocketCardKey(ReservedProductIds.personhood(tld), PocketCardId("loyalty"))))
    }
}
