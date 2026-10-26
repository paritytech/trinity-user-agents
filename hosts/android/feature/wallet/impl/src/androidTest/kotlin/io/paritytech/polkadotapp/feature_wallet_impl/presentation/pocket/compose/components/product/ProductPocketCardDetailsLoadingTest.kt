package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import android.webkit.WebView
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownRequest
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ShownProduct
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.LocalNavAnimatedVisibilityScope
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ProductPocketCardDetailsLoadingTest {
    @get:Rule
    val compose = createComposeRule()

    private class FakeSession : SpaHostSession {
        val faceRequests = MutableSharedFlow<FaceShownRequest>(extraBufferCapacity = 1)

        override val webView = MutableStateFlow<WebView?>(null)
        override val currentUrl = MutableStateFlow("")
        override val loadProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Completed)
        override val title = MutableStateFlow("")
        override val faceShownRequests get() = faceRequests
        override fun pauseConnections() = Unit
        override fun resumeConnections() = Unit
    }

    private val card = PocketCardUiModel.ProductCard(
        key = PocketCardKey(ProductId.fromStoredValue("pocketdemo.dot"), PocketCardId("loyalty")),
        title = "Loyalty",
        pinned = false,
    )

    @Composable
    private fun Details(session: SpaHostSession?) {
        PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
            ProductPocketCardDetails(
                card = card,
                bindings = ProductFaceBindings(
                    face = MutableStateFlow(null),
                    onFaceAction = { _, _ -> },
                    imageResolver = JsImageResolver { null },
                ),
                shownProduct = session?.let { ShownProduct(card.key.launchUrl(), it) },
                openingFaceShown = true,
                cardIndex = 0,
                onSettled = {},
                onBack = {},
            )
        }
    }

    // Products draw their own loading state; a host spinner shown for under a second reads as a glitch.
    @Test
    fun aProductStillOnItsWayGetsTheHostSpinnerOnlyOnceItIsClearlyLate() {
        compose.mainClock.autoAdvance = false
        compose.setContent { Details(session = null) }

        compose.mainClock.advanceTimeBy(PRODUCT_PROGRESS_DELAY.inWholeMilliseconds - 100)
        compose.onNodeWithTag(PocketTestTags.PRODUCT_LOADING).assertDoesNotExist()

        compose.mainClock.advanceTimeBy(200)
        compose.onNodeWithTag(PocketTestTags.PRODUCT_LOADING).assertExists()
    }

    // A reopened product is on screen while its card moves. Its requests wait for the card to settle:
    // applied at once they would move the card mid-flight, and refused they would be lost.
    @Test
    fun aRequestFromAProductWhoseCardIsMovingIsHeldUntilTheCardSettles() {
        val session = FakeSession()
        val arrival = showArriving(session)
        assertEquals("not listening while the card moved", 1, session.faceRequests.subscriptionCount.value)

        val reply = requestFaceHidden(session)
        assertFalse("applied while the card was moving", reply.isCompleted)

        compose.mainClock.advanceTimeUntil(timeoutMillis = 3_000) { reply.isCompleted }
        assertTrue("applied before the card settled", arrival.isIdle)
        assertEquals(FaceShownAnswer.APPLIED, runBlocking { reply.await() })
    }

    // A card closed before it settles never moves its face, so the product is told at once instead of
    // waiting out its call.
    @Test
    fun aRequestHeldForACardThatLeavesBeforeSettlingIsNotPresented() {
        val session = FakeSession()
        val arrival = showArriving(session)
        val reply = requestFaceHidden(session)

        arrival.targetState = false
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()

        assertTrue("left unanswered when the card left", reply.isCompleted)
        assertEquals(FaceShownAnswer.NOT_PRESENTED, runBlocking { reply.await() })
    }

    private fun showArriving(session: SpaHostSession): MutableTransitionState<Boolean> {
        val arrival = MutableTransitionState(false).apply { targetState = true }
        compose.mainClock.autoAdvance = false
        compose.setContent {
            AnimatedVisibility(visibleState = arrival) {
                CompositionLocalProvider(LocalNavAnimatedVisibilityScope provides this) { Details(session) }
            }
        }
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        return arrival
    }

    private fun requestFaceHidden(session: FakeSession): CompletableDeferred<FaceShownAnswer> {
        val reply = CompletableDeferred<FaceShownAnswer>()
        assertTrue(session.faceRequests.tryEmit(FaceShownRequest(shown = false, reply = reply)))
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        return reply
    }
}
