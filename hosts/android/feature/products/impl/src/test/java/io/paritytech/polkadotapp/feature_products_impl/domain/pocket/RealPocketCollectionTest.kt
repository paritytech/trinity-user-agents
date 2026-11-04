package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_impl.data.pocket.PocketCardRepository
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.RendererNode

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

    @Test
    fun `removing a privileged card leaves it in the collection`() = runTest {
        assertTrue(collection.removeCard(humanity.card.key).isSuccess)

        assertEquals(listOf(humanity.card), collection.observeCards().first())
    }

    @Test
    fun `removing an added card takes its face with it`() = runTest {
        collection.addCard(loyalty)

        assertTrue(collection.removeCard(loyalty.card.key).isSuccess)

        assertEquals(listOf(humanity.card), collection.observeCards().first())
        assertNull(collection.cachedFace(loyalty.card.key))
    }

    // The screen shows the failure; a removal that did not happen must not read as done.
    @Test
    fun `a removal the core could not make is reported as a failure`() = runTest {
        val failing = object : PocketCardRepository by repository {
            override suspend fun remove(key: PocketCardKey) = error("runtime unavailable")
        }

        val offline = RealPocketCollection(FakePinnedPocketCards(emptyList()), failing)

        assertTrue(offline.removeCard(loyalty.card.key).isFailure)
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

    // Reading a kept face goes through the runtime, which may not have started yet.
    @Test
    fun `a pinned card shows its bundled face when the kept one cannot be read`() = runTest {
        val unreadable = object : PocketCardRepository by repository {
            override suspend fun face(key: PocketCardKey): RendererNode? = error("runtime unavailable")
        }

        val offline = RealPocketCollection(FakePinnedPocketCards(listOf(humanity)), unreadable)

        assertEquals(humanity.face, offline.cachedFace(humanity.card.key))
    }
}
