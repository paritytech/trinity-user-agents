package io.paritytech.polkadotapp.common.presentation.statusBar

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.docklet.DockletBackdrop
import io.paritytech.polkadotapp.design.components.docklet.dockletBackdrop
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val PlateHeight = 24.dp
private val PlateBlurSigma = 6.dp
private const val SCROLLED_PLATE_ALPHA = 0.05f
private const val HIGHLIGHTED_PLATE_ALPHA = 0.1f
private const val PLATE_FADE_MILLIS = 200

@Immutable
data class IdBarPlateBackground(
    val scrolled: Boolean,
    val backdrop: DockletBackdrop?,
)

val LocalIdBarPlateBackground = compositionLocalOf { IdBarPlateBackground(scrolled = false, backdrop = null) }

@Composable
fun IdBarPlate(
    modifier: Modifier = Modifier,
    highlighted: Boolean,
    contentPadding: PaddingValues,
    content: @Composable RowScope.() -> Unit,
) {
    val background = LocalIdBarPlateBackground.current
    val shape = PolkadotTheme.shapes.extraMedium
    val scrolledTint = PolkadotTheme.colors.bg.surface.overlay
    val highlightedTint = PolkadotTheme.colors.fg.primary
    val scrolled = animateFloatAsState(
        targetValue = if (background.scrolled && !highlighted) 1f else 0f,
        animationSpec = tween(PLATE_FADE_MILLIS),
        label = "idBarScrolledPlate",
    )
    val highlight = animateFloatAsState(
        targetValue = if (highlighted) 1f else 0f,
        animationSpec = tween(PLATE_FADE_MILLIS),
        label = "idBarHighlightedPlate",
    )

    Box(modifier = modifier) {
        Row(
            modifier = Modifier
                .requiredHeight(PlateHeight)
                .dockletBackdrop(background.backdrop, shape) { PlateBlurSigma * scrolled.value }
                .drawWithCache {
                    val outline = shape.createOutline(size, layoutDirection, this)
                    onDrawBehind {
                        drawOutline(outline, scrolledTint.copy(alpha = SCROLLED_PLATE_ALPHA * scrolled.value))
                        drawOutline(outline, highlightedTint.copy(alpha = HIGHLIGHTED_PLATE_ALPHA * highlight.value))
                    }
                }
                .padding(contentPadding),
            verticalAlignment = Alignment.CenterVertically,
            content = content,
        )
    }
}
