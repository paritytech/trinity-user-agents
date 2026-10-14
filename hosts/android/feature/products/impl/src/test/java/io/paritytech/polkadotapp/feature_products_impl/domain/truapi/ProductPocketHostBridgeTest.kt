package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.FakePinnedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.InMemoryPocketCardRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RealPocketCollection
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.addedCard
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.gameProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.personhoodProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.pinnedCard
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.NativePocketRemoval
import uniffi.truapi.PocketCard as NativePocketCard

class ProductPocketHostBridgeTest {
    private val humanity = pinnedCard(personhoodProduct, "humanity")
    private val loyalty = addedCard(gameProduct, "loyalty")
    private val store = RealPocketCollection(FakePinnedPocketCards(listOf(humanity)), InMemoryPocketCardRepository())

    // The execution scope the bridge hands its work to; a separate scope, as in production, so the
    // test's own scope does not wait on the never-ending collector.
    private fun TestScope.executionScope() = CoroutineScope(StandardTestDispatcher(testScheduler))

    @Test
    fun `serves only the calling product's cards and republishes on every change`() = runTest {
        val scope = executionScope()
        val published = mutableListOf<List<NativePocketCard>>()
        val bridge = ProductPocketHostBridge(gameProduct, store, scope)

        bridge.start { published += it }
        advanceUntilIdle()
        store.addCard(loyalty)
        advanceUntilIdle()

        assertEquals(listOf(emptyList(), listOf(NativePocketCard("loyalty", privileged = false))), published)
        assertEquals(listOf(NativePocketCard("loyalty", privileged = false)), bridge.listCards())
        scope.cancel()
    }

    @Test
    fun `a privileged card shows its flag so the core can refuse its removal itself`() = runTest {
        val scope = executionScope()
        val bridge = ProductPocketHostBridge(personhoodProduct, store, scope)

        bridge.start {}
        advanceUntilIdle()

        assertEquals(listOf(NativePocketCard("humanity", privileged = true)), bridge.listCards())
        assertEquals(NativePocketRemoval.PRIVILEGED, bridge.removeCard("humanity"))
        assertEquals(listOf(NativePocketCard("humanity", privileged = true)), bridge.listCards())
        scope.cancel()
    }

    // The core reads listCards right after removeCard returns and republishes that answer, so a
    // removal that is still in flight would hand the product its old collection back.
    @Test
    fun `the card is gone from the store and the list by the time removeCard returns`() = runTest {
        val scope = executionScope()
        store.addCard(loyalty)
        val bridge = ProductPocketHostBridge(gameProduct, store, scope)
        bridge.start {}
        advanceUntilIdle()

        assertEquals(NativePocketRemoval.REMOVED, bridge.removeCard("loyalty"))
        assertEquals(NativePocketRemoval.ABSENT, bridge.removeCard("never-added"))

        assertEquals(emptyList<NativePocketCard>(), bridge.listCards())
        assertEquals(listOf(humanity.card), store.observeCards().first())
        scope.cancel()
    }

    // The execution the republish lands on is a native handle the bridge's owner closes. A collector
    // outliving that close calls into freed memory from a scope with no handler, taking the process
    // with it; a re-attach would then leave two collectors on one product.
    @Test
    fun `nothing is republished once the bridge is stopped`() = runTest {
        val scope = executionScope()
        val published = mutableListOf<List<NativePocketCard>>()
        val bridge = ProductPocketHostBridge(gameProduct, store, scope)
        bridge.start { published += it }
        advanceUntilIdle()

        bridge.stop()
        store.addCard(loyalty)
        advanceUntilIdle()

        assertEquals(listOf(emptyList<NativePocketCard>()), published)
        scope.cancel()
    }

    @Test
    fun `another product's removal request cannot touch this product's card`() = runTest {
        val scope = executionScope()
        store.addCard(loyalty)
        val otherProduct = ProductId.fromStoredValue("other.dot")
        val bridge = ProductPocketHostBridge(otherProduct, store, scope)
        bridge.start {}
        advanceUntilIdle()

        bridge.removeCard("loyalty")
        advanceUntilIdle()

        assertEquals(listOf(humanity.card, loyalty.card), store.observeCards().first())
        scope.cancel()
    }
}
