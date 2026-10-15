package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import android.webkit.WebView
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
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
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.LocalNavAnimatedVisibilityScope
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
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

    private class FakeSession(progress: DotNsLoadProgress) : SpaHostSession {
        val faceRequests = MutableSharedFlow<FaceShownRequest>(extraBufferCapacity = 1)

        override val webView = MutableStateFlow<WebView?>(null)
        override val currentUrl = MutableStateFlow("")
        override val loadProgress = MutableStateFlow(progress)
        override val title = MutableStateFlow("")
        override val faceShownRequests get() = faceRequests
        override fun pauseConnections() = Unit
        override fun resumeConnections() = Unit
    }

    @Composable
    private fun Details(session: SpaHostSession?) {
        PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
            ProductPocketCardDetails(
                card = PocketCardUiModel.ProductCard(
                    key = PocketCardKey(ProductId.fromStoredValue("pocketdemo.dot"), PocketCardId("loyalty")),
                    title = "Loyalty",
                    pinned = false,
                ),
                bindings = ProductFaceBindings(
                    face = MutableStateFlow(null),
                    onFaceAction = { _, _ -> },
                    imageResolver = JsImageResolver { null },
                ),
                session = session,
                openingFaceShown = true,
                cardIndex = 0,
                onSettled = {},
                onBack = {},
            )
        }
    }

    // Products draw their own loading state, so a host spinner in front of them is a second layer of
    // loading. It is kept for a product that is clearly late; one shown for under a second reads as
    // a glitch.
    @Test
    fun aProductStillOnItsWayGetsTheHostSpinnerOnlyOnceItIsClearlyLate() {
        compose.mainClock.autoAdvance = false
        compose.setContent { Details(session = null) }

        compose.mainClock.advanceTimeBy(1_400)
        compose.onNodeWithTag(PocketTestTags.PRODUCT_LOADING).assertDoesNotExist()

        compose.mainClock.advanceTimeBy(200)
        compose.onNodeWithTag(PocketTestTags.PRODUCT_LOADING).assertExists()
    }

    // There is one WebView per product, and showing it takes it from wherever it was. A screen fading
    // out while the next card arrives over the same product would take that card's page away.
    @Test
    fun aScreenBeingClosedShowsNeitherThePageNorTheSpinner() {
        val session = FakeSession(DotNsLoadProgress.Completed)
        var shown by mutableStateOf(true)
        compose.mainClock.autoAdvance = false
        compose.setContent {
            AnimatedVisibility(visible = shown) {
                CompositionLocalProvider(LocalNavAnimatedVisibilityScope provides this) { Details(session) }
            }
        }
        compose.mainClock.advanceTimeBy(2_000)
        compose.onNodeWithTag(PocketTestTags.PRODUCT_PAGE).assertExists()

        shown = false
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()

        compose.onNodeWithTag(PocketTestTags.PRODUCT_CARD).assertExists()
        compose.onNodeWithTag(PocketTestTags.PRODUCT_PAGE).assertDoesNotExist()
        compose.onNodeWithTag(PocketTestTags.PRODUCT_LOADING).assertDoesNotExist()
    }

    // A product opened again is on screen while its card is still moving. Its requests to fold the
    // face wait for the card to settle: applied at once they would move the card mid-flight, and
    // turned away they would be lost, since the product does not ask again.
    @Test
    fun aRequestFromAProductWhoseCardIsMovingIsHeldUntilTheCardSettles() {
        val session = FakeSession(DotNsLoadProgress.Completed)
        val arrival = MutableTransitionState(false).apply { targetState = true }
        compose.mainClock.autoAdvance = false
        compose.setContent {
            AnimatedVisibility(visibleState = arrival) {
                CompositionLocalProvider(LocalNavAnimatedVisibilityScope provides this) { Details(session) }
            }
        }
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertEquals("nobody listening, so the request is turned away", 1, session.faceRequests.subscriptionCount.value)

        val reply = CompletableDeferred<FaceShownAnswer>()
        assertTrue(session.faceRequests.tryEmit(FaceShownRequest(shown = false, reply = reply)))
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertFalse("applied while the card was moving", reply.isCompleted)

        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        assertEquals(FaceShownAnswer.APPLIED, reply.getCompleted())
    }
}
