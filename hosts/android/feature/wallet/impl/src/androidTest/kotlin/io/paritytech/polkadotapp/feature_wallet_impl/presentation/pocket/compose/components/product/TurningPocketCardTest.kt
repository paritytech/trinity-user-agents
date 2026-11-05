package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeUp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.ProductId

// A slow swipe ends at rest, so it turns the card over once rather than flinging it on.
private const val SLOW_SWIPE_MILLIS = 1_000L

@RunWith(AndroidJUnit4::class)
class TurningPocketCardTest {
    @get:Rule
    val compose = createComposeRule()

    private val presses = mutableListOf<String>()
    // The back is one button filling the card, so a tap anywhere on the front lies over it.
    private val back = MutableStateFlow<JsWidget?>(
        JsWidget.Button(text = "Back", onClick = "back", modifiers = listOf(JsModifier.FillMaxWidth(), JsModifier.FillMaxHeight())),
    )

    private fun bindings(face: MutableStateFlow<JsWidget?>) = ProductFaceBindings(
        face = face,
        onFaceAction = { actionId, _ -> presses += actionId },
        imageResolver = JsImageResolver { null },
    )

    private fun showCard() = compose.setContent {
        PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
            TurningPocketCard(
                card = PocketCardUiModel.ProductCard(
                    key = PocketCardKey(ProductId.parse("pocketdemo.dot").getOrThrow(), PocketCardId("loyalty")),
                    title = "Loyalty",
                    pinned = false,
                ),
                front = bindings(MutableStateFlow(JsWidget.Text(text = "Front"))),
                back = bindings(back),
                leaving = false,
            )
        }
    }

    private fun turnable() = SemanticsMatcher.keyIsDefined(SemanticsActions.CustomActions)

    // The face turned away is drawn too, so a screen reader or a tap must not reach it.
    @Test
    fun aTapAndAScreenReaderReachOnlyTheFaceUp() {
        showCard()

        compose.onNodeWithTag(PocketTestTags.TURNING_CARD).performTouchInput { click(center) }

        assertEquals(emptyList<String>(), presses)
        compose.onAllNodesWithText("Back").assertCountEquals(0)
    }

    // A vertical drag folds the opened card away; only a sideways one turns it.
    @Test
    fun aSidewaysSwipeTurnsTheCardAndAVerticalOneDoesNot() {
        showCard()
        val card = compose.onNodeWithTag(PocketTestTags.TURNING_CARD)

        card.performTouchInput { swipeUp() }
        compose.onNodeWithText("Front").assertExists()

        card.performTouchInput { swipeLeft(durationMillis = SLOW_SWIPE_MILLIS) }
        compose.onNodeWithText("Back").performClick()
        compose.onAllNodesWithText("Front").assertCountEquals(0)
        assertEquals(listOf("back"), presses)
    }

    // A gesture-only feature would be closed to screen-reader users.
    @Test
    fun turnOverShowsTheBack() {
        showCard()

        val turnOver = compose.onNodeWithTag(PocketTestTags.TURNING_CARD).fetchSemanticsNode().config[SemanticsActions.CustomActions]
        compose.runOnIdle { turnOver.single().action() }

        compose.onNodeWithText("Back").assertExists()
    }

    @Test
    fun aCardWithoutABackCannotBeTurned() {
        back.value = null
        showCard()
        val card = compose.onNodeWithTag(PocketTestTags.TURNING_CARD)

        card.performTouchInput { swipeLeft(durationMillis = SLOW_SWIPE_MILLIS) }

        compose.onNodeWithText("Front").assertExists()
        card.assert(!turnable())
    }

    // A back the product takes away leaves nothing to show, so the front returns without a turn.
    @Test
    fun aBackThatGoesAwayWhileShownBringsTheFrontBack() {
        showCard()
        compose.onNodeWithTag(PocketTestTags.TURNING_CARD).performTouchInput { swipeLeft(durationMillis = SLOW_SWIPE_MILLIS) }
        compose.onNodeWithText("Back").assertExists()

        back.value = null

        compose.onNodeWithText("Front").assertExists()
    }
}
