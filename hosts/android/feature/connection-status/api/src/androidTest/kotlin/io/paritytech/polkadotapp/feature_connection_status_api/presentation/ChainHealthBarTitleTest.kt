package io.paritytech.polkadotapp.feature_connection_status_api.presentation

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.isPopup
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthItemModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.IndicatorRow
import kotlinx.collections.immutable.toImmutableList
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

private const val TITLE_TAG = "status_bar_title"
private const val PIXEL_TOLERANCE = 0.5f
private val LONG_TITLE = "A product whose title runs far past the indicators ".repeat(4)

@RunWith(AndroidJUnit4::class)
class ChainHealthBarTitleTest {
    @get:Rule
    val compose = createComposeRule()

    private val model = ChainHealthIndicatorsModel(
        IndicatorRow.entries.map { ChainHealthItemModel(it, ChainHealthIndicator.Healthy(liveness = null)) }.toImmutableList()
    )

    private var title by mutableStateOf<String?>(null)
    private var tooltipVisible by mutableStateOf(false)
    private var barGap = 0.dp

    @Test
    fun aLongTitleEndsBeforeTheIndicatorsWithAGap() {
        setBar()
        title = LONG_TITLE
        compose.waitForIdle()

        val titleRight = compose.onNodeWithTag(TITLE_TAG).fetchSemanticsNode().boundsInWindow.right
        val indicatorsLeft = indicatorBounds().left
        val gap = with(compose.density) { barGap.toPx() }

        assertTrue(
            "title ends at $titleRight px, indicators start at $indicatorsLeft px",
            titleRight + gap <= indicatorsLeft + PIXEL_TOLERANCE,
        )
    }

    @Test
    fun theIndicatorsKeepTheirPlaceWithAndWithoutATitle() {
        setBar()
        val withoutTitle = indicatorBounds()

        title = LONG_TITLE
        compose.waitForIdle()

        assertEquals(withoutTitle, indicatorBounds())
    }

    @Test
    fun theTooltipKeepsItsPlaceWithATitle() {
        setBar()
        tooltipVisible = true
        compose.waitForIdle()
        val withoutTitle = tooltipBounds()

        title = LONG_TITLE
        compose.waitForIdle()

        assertEquals(withoutTitle, tooltipBounds())
    }

    private fun setBar() {
        compose.setContent {
            CompositionLocalProvider(LocalInspectionMode provides true) {
                PolkadotTheme {
                    barGap = PolkadotTheme.spacings.small
                    ChainHealthBar(
                        model = model,
                        tooltipVisible = tooltipVisible,
                        onTooltipDismiss = {},
                        title = {
                            title?.let {
                                BasicText(
                                    modifier = Modifier.testTag(TITLE_TAG),
                                    text = it,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        },
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private fun indicatorBounds(): Rect {
        val nodes = compose.onAllNodes(SemanticsMatcher.keyIsDefined(SemanticsProperties.ContentDescription))
            .fetchSemanticsNodes()
        assertEquals(IndicatorRow.entries.size, nodes.size)

        return nodes.map { it.boundsInWindow }.reduce { union, bounds ->
            Rect(
                left = minOf(union.left, bounds.left),
                top = minOf(union.top, bounds.top),
                right = maxOf(union.right, bounds.right),
                bottom = maxOf(union.bottom, bounds.bottom),
            )
        }
    }

    private fun tooltipBounds(): Rect = compose.onAllNodes(isPopup()).fetchSemanticsNodes().single().boundsInWindow
}
