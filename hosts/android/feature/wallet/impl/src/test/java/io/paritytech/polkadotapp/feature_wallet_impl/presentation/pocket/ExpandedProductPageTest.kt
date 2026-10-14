package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import android.webkit.WebView
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ExpandedProductPageTest {
    private class FakeSession : SpaHostSession {
        override val webView = MutableStateFlow<WebView?>(null)
        override val currentUrl = MutableStateFlow("")
        override val loadProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle)
        override val title = MutableStateFlow("")

        var paused = 0
        var resumed = 0

        override fun pauseConnections() {
            paused++
        }

        override fun resumeConnections() {
            resumed++
        }
    }

    private val opened = mutableListOf<Triple<String, CoroutineScope, FakeSession>>()

    private val sessions: List<FakeSession> get() = opened.map { it.third }

    // The screen's own scope, separate from the test's, so a page left open does not hold runTest open.
    private fun TestScope.screenScope() = CoroutineScope(StandardTestDispatcher(testScheduler))

    private fun page(screenScope: CoroutineScope) = ExpandedProductPage(screenScope) { scope, url ->
        FakeSession().also { opened += Triple(url, scope, it) }
    }

    @Test
    fun `opening a card hosts its product and hands the session to the screen`() = runTest {
        val page = page(screenScope())

        page.open("https://game.dot?card=loyalty")

        assertEquals(listOf("https://game.dot?card=loyalty"), opened.map { it.first })
        assertTrue(page.session.value is FakeSession)
    }

    // The product costs a WebView and a page load to build, and a user who looks at a card usually
    // looks at it again. It is kept alive rather than rebuilt, and paused so it stops working unseen.
    @Test
    fun `closing the card keeps its product alive but paused and off screen`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")

        page.close()

        assertNull(page.session.value)
        assertTrue(opened.single().second.isActive)
        assertEquals(1, sessions.single().paused)
    }

    @Test
    fun `reopening the same card shows the product still held instead of building a second one`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")
        page.close()

        page.open("https://game.dot?card=loyalty")

        assertEquals(1, opened.size)
        assertEquals(sessions.single(), page.session.value)
        assertEquals(1, sessions.single().resumed)
    }

    // Only one product is ever held, so a second card cannot leave the first one's WebView behind.
    @Test
    fun `opening another card takes the held product down`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")
        page.close()

        page.open("https://shop.dot?card=points")

        val (first, second) = opened.map { it.second }
        assertFalse(first.isActive)
        assertTrue(second.isActive)
    }

    @Test
    fun `opening another card while one is on screen leaves only the new product running`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")

        page.open("https://shop.dot?card=points")

        val (first, second) = opened.map { it.second }
        assertFalse(first.isActive)
        assertTrue(second.isActive)
    }

    // The screen asks for the product once the card has settled, and it asks again on every
    // recomposition that follows. Re-opening would tear down a live WebView and start over.
    @Test
    fun `asking again for the product already on screen changes nothing`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")

        page.open("https://game.dot?card=loyalty")

        assertEquals(1, opened.size)
        assertTrue(opened.single().second.isActive)
        assertEquals(0, sessions.single().resumed)
    }

    @Test
    fun `closing a card that hosts nothing is harmless`() = runTest {
        val page = page(screenScope())

        page.close()

        assertTrue(opened.isEmpty())
        assertNull(page.session.value)
    }

    @Test
    fun `leaving the screen takes the open product with it`() = runTest {
        val screen = screenScope()
        page(screen).open("https://game.dot?card=loyalty")

        screen.cancel()

        assertFalse(opened.single().second.isActive)
    }

    // A held product outlives the card being closed, so only the screen going away can end it.
    @Test
    fun `leaving the screen takes a held product with it`() = runTest {
        val screen = screenScope()
        val page = page(screen)
        page.open("https://game.dot?card=loyalty")
        page.close()

        screen.cancel()

        assertFalse(opened.single().second.isActive)
    }

    // Collapsing a card keeps its product warm for the next tap. A card that has left the
    // collection has no next tap, and its WebView, worker reference and chain sockets would sit
    // there until the screen itself goes.
    @Test
    fun `releasing a card takes its product down rather than keeping it warm`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")
        page.close()

        page.release()

        assertFalse(opened.single().second.isActive)
        assertNull(page.session.value)
    }

    @Test
    fun `a card opened again after being released is hosted afresh`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")
        page.release()

        page.open("https://game.dot?card=loyalty")

        assertEquals(2, opened.size)
        assertTrue(opened.last().second.isActive)
    }

    // The collection is what says a card still exists. A card removed while its product sat parked
    // behind a closed card would otherwise keep that product until the screen itself went.
    @Test
    fun `a product parked for a card that has left the collection is given up`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")
        page.close()

        page.keepOnly { it == "https://shop.dot?card=points" }

        assertFalse(opened.single().second.isActive)
    }

    @Test
    fun `a product parked for a card the collection still holds is kept`() = runTest {
        val page = page(screenScope())
        page.open("https://game.dot?card=loyalty")
        page.close()

        page.keepOnly { it == "https://game.dot?card=loyalty" }

        assertTrue(opened.single().second.isActive)
    }
}
