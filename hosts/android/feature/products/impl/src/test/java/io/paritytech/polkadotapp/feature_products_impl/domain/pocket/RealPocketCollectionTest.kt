package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketRemoval
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketRemoveError
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RealPocketCollectionTest {
    private val humanity = pinnedCard(personhoodProduct, "humanity")
    private val loyalty = addedCard(gameProduct, "loyalty")
    private val repository = InMemoryPocketCardRepository()
    private val collection = RealPocketCollection(FakePinnedPocketCards(listOf(humanity)), repository)

    @Test
    fun `pinned cards come first and are present before anything was added`() = runTest {
        assertEquals(listOf(humanity.card), collection.observeCards().first())

        collection.addCard(loyalty)

        assertEquals(listOf(humanity.card, loyalty.card), collection.observeCards().first())
    }

    @Test
    fun `removing a privileged card is refused and changes nothing`() = runTest {
        val outcome = collection.removeCard(humanity.card.key)

        assertEquals(PocketRemoveError.Privileged, outcome.exceptionOrNull())
        assertEquals(listOf(humanity.card), collection.observeCards().first())
        assertEquals(humanity.face, collection.cachedFace(humanity.card.key))
    }

    @Test
    fun `removing an added card drops it and its cached face`() = runTest {
        collection.addCard(loyalty)

        assertTrue(collection.removeCard(loyalty.card.key).isSuccess)

        assertEquals(listOf(humanity.card), collection.observeCards().first())
        assertNull(collection.cachedFace(loyalty.card.key))
    }

    @Test
    fun `removing an absent card succeeds, so a product retry is harmless`() = runTest {
        assertTrue(collection.removeCard(cardKey(gameProduct, "never-added")).isSuccess)
    }

    @Test
    fun `a card added twice keeps one entry with the newest face`() = runTest {
        collection.addCard(loyalty)
        val refreshed = loyalty.copy(face = faceOf("newer"))

        collection.addCard(refreshed)

        assertEquals(listOf(humanity.card, loyalty.card), collection.observeCards().first())
        assertEquals(refreshed.face, collection.cachedFace(loyalty.card.key))
    }

    @Test
    fun `only the host places privileged cards`() = runTest {
        val outcome = runCatching { collection.addCard(humanity) }

        assertTrue(outcome.exceptionOrNull() is IllegalArgumentException)
    }

    // The core asks the host to tell a card it took out from one it never held, so the product's
    // retry of a removal reads as already gone rather than as a second removal.
    @Test
    fun `a removal reports whether the card was there to remove`() = runTest {
        collection.addCard(loyalty)

        assertEquals(PocketRemoval.REMOVED, collection.removeCard(loyalty.card.key).getOrThrow())
        assertEquals(PocketRemoval.ABSENT, collection.removeCard(loyalty.card.key).getOrThrow())
        assertEquals(PocketRemoveError.Privileged, collection.removeCard(humanity.card.key).exceptionOrNull())
    }

    // The bundled face is what a privileged card shows before its product has ever drawn one. Once it
    // has, that is the face the card wears at the next cold start, rather than reverting to the stub.
    @Test
    fun `a pinned card keeps the newest face its product drew across a restart`() = runTest {
        collection.cacheFace(humanity.card.key, faceOf("live"))

        val afterRestart = RealPocketCollection(FakePinnedPocketCards(listOf(humanity)), repository)

        assertEquals(faceOf("live"), afterRestart.cachedFace(humanity.card.key))
    }

    @Test
    fun `the face kept for a pinned card does not turn it into one the user added`() = runTest {
        collection.cacheFace(humanity.card.key, faceOf("live"))

        assertEquals(listOf(humanity.card), collection.observeCards().first())
    }

    // A product redraws whenever it likes, including between the user pressing Remove and the row
    // going. A face kept then must not put the card back in the collection.
    @Test
    fun `a face arriving after a removal does not bring the card back`() = runTest {
        collection.addCard(loyalty)
        collection.removeCard(loyalty.card.key)

        collection.cacheFace(loyalty.card.key, faceOf("late"))

        assertEquals(listOf(humanity.card), collection.observeCards().first())
    }

    @Test
    fun `a pinned card with nothing drawn yet still shows the face bundled with the app`() = runTest {
        assertEquals(humanity.face, collection.cachedFace(humanity.card.key))
    }

    // Listing the pinned cards waits for the network's name, which offline never arrives. A removal
    // must not wait with it: the core asks for one inline, on a dispatcher thread it cannot spare,
    // and a face read backs every card on screen.
    @Test
    fun `a removal and a face read do not wait for the pinned cards to be listable`() = runTest {
        val offline = RealPocketCollection(FakePinnedPocketCards(listOf(humanity), listable = false), repository)
        offline.addCard(loyalty)

        assertEquals(PocketRemoval.REMOVED, offline.removeCard(loyalty.card.key).getOrThrow())
        assertEquals(PocketRemoveError.Privileged, offline.removeCard(humanity.card.key).exceptionOrNull())
        assertEquals(humanity.face, offline.cachedFace(humanity.card.key))
    }
}
