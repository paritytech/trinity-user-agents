package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlayInsets
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatInputSlotRenderer
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatInputUiState
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.math.abs

/**
 * A product's docked room footer card and the global game pill: the pill is drawn over the bottom of every
 * screen, so the chat lifts the card by what the overlays report, and the card stays above the pill.
 */
@RunWith(AndroidJUnit4::class)
class ChatInputRowGamePillTest {
    @get:Rule
    val compose = createComposeRule()

    @After
    fun noOverlay() {
        ChatOverlayInsets.reportBottom(0)
    }

    @Test
    fun aDockedCardSitsAboveTheGamePill() {
        val pill = drawWithOverlay(pillPx = 240)

        assertNear(pill.screenBottom - 240f, pill.cardBottom)
    }

    @Test
    fun withNoOverlayTheDockedCardSitsOnTheBottom() {
        val pill = drawWithOverlay(pillPx = 0)

        assertNear(pill.screenBottom, pill.cardBottom)
    }

    private class Drawn(val screenBottom: Float, val cardBottom: Float)

    private fun drawWithOverlay(pillPx: Int): Drawn {
        ChatOverlayInsets.reportBottom(pillPx)
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                Box(Modifier.size(SCREEN_SIZE).testTag(SCREEN)) {
                    Box(Modifier.align(Alignment.BottomCenter)) {
                        ChatInputRow(
                            inputState = ChatInputUiState.ExtensionCard(DockedCard),
                            username = "",
                            onClearReply = {},
                            onClearEdit = {},
                            onMessageChange = {},
                            onSendMessageClick = {},
                            onPayClick = {},
                            onAttachClick = {},
                            onAcceptChatRequest = {},
                            onDeclineChatRequest = {},
                            onUnblockUserClick = {},
                            onAllowIdentitySigningClick = {},
                            onDeclineIdentitySigningClick = {},
                            dockletHiddenFraction = { 1f },
                            onHeightChanged = {},
                        )
                    }
                }
            }
        }
        compose.waitForIdle()
        return Drawn(
            screenBottom = compose.onNodeWithTag(SCREEN).getBoundsInRoot().bottom.toPx(),
            cardBottom = compose.onNodeWithTag(CARD).getBoundsInRoot().bottom.toPx(),
        )
    }

    private fun Dp.toPx(): Float = value * compose.density.density

    private fun assertNear(expected: Float, actual: Float) {
        assertEquals("card bottom $actual, expected $expected", true, abs(expected - actual) <= 1f)
    }

    /** A docked room footer card: a product's 80 tall card. */
    private object DockedCard : CustomChatInputSlotRenderer {
        @Composable
        override fun drawInputSlot() {
            Box(Modifier.fillMaxWidth().height(80.dp).testTag(CARD))
        }
    }

    private companion object {
        val SCREEN_SIZE = 600.dp
        const val SCREEN = "screen"
        const val CARD = "docked card"
    }
}
