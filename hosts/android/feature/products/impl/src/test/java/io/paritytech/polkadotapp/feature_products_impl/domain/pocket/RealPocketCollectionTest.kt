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
    private val repository = InMemoryPocketCardRepository(pinned = listOf(humanity.card))
    private val collection = RealPocketCollection(FakePinnedPocketCards(listOf(humanity)), repository)

    @Test
    fun `the collection is the one the core keeps`() = runTest {
        collection.addCard(loyalty)

        assertEquals(listOf(humanity.card, loyalty.card), collection.observeCards().first())
    }

    // Callers tell a refusal apart from a removal by the error, so the core's answer has to arrive as one.
    @Test
    fun `removing a privileged card is refused and changes nothing`() = runTest {
        val outcome = collection.removeCard(humanity.card.key)

        assertEquals(PocketRemoveError.Privileged, outcome.exceptionOrNull())
        assertEquals(listOf(humanity.card), collection.observeCards().first())
    }

    // The core asks the host to tell a card it took out from one it never held, so the product's
    // retry of a removal reads as already gone rather than as a second removal.
    @Test
    fun `a removal reports whether the card was there to remove`() = runTest {
        collection.addCard(loyalty)

        assertEquals(PocketRemoval.REMOVED, collection.removeCard(loyalty.card.key).getOrThrow())
        assertEquals(PocketRemoval.ABSENT, collection.removeCard(loyalty.card.key).getOrThrow())
        assertNull(collection.cachedFace(loyalty.card.key))
    }

    @Test
    fun `only the host places privileged cards`() = runTest {
        val outcome = runCatching { collection.addCard(humanity) }

        assertTrue(outcome.exceptionOrNull() is IllegalArgumentException)
    }

    @Test
    fun `an added card shows the face it was approved by`() = runTest {
        collection.addCard(loyalty)

        assertEquals(loyalty.face, collection.cachedFace(loyalty.card.key))
    }

    // The bundled face is what a pinned card shows before its product has ever drawn one.
    @Test
    fun `a pinned card with nothing drawn yet still shows the face bundled with the app`() = runTest {
        assertEquals(humanity.face, collection.cachedFace(humanity.card.key))
    }
}
