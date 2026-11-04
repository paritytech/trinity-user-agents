package io.paritytech.polkadotapp.app.root.presentation.root.compose

import android.view.View
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.statusBar.IdBarPlateBackground
import io.paritytech.polkadotapp.common.presentation.statusBar.LocalIdBarPlateBackground
import io.paritytech.polkadotapp.common.presentation.statusBar.StatusBarScrollState
import io.paritytech.polkadotapp.design.components.docklet.DockletBackdrop
import io.paritytech.polkadotapp.design.components.docklet.dockletBackdrop
import io.paritytech.polkadotapp.design.components.docklet.rememberDockletBackdrop
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.ChainHealthBarDefaults

private val IdBarSlide = 40.dp
private const val ID_BAR_SLIDE_MILLIS = 300
private val ScrimBlurSigma = 2.dp
private const val SCRIM_TOP_ALPHA = 0.55f
private const val SCRIM_MIDDLE_ALPHA = 0.25f
private const val SCRIM_MIDDLE_STOP = 0.6f

@Composable
fun IdBar(
    scroll: StatusBarScrollState,
    backdropSource: View,
    showsScrim: Boolean,
    pinned: Boolean,
    bar: @Composable () -> Unit,
) {
    val backdrop = rememberDockletBackdrop(backdropSource)
    val slidePx = with(LocalDensity.current) { IdBarSlide.roundToPx() }
    val plateBackground = remember(scroll.contentUnderBar, backdrop) {
        IdBarPlateBackground(scrolled = scroll.contentUnderBar, backdrop = backdrop)
    }

    Box {
        if (showsScrim) {
            IdBarScrim(backdrop = backdrop, blurred = scroll.contentUnderBar)
        }

        AnimatedVisibility(
            visible = pinned || !scroll.idBarHidden,
            enter = slideInVertically(tween(ID_BAR_SLIDE_MILLIS)) { -slidePx } + fadeIn(tween(ID_BAR_SLIDE_MILLIS)),
            exit = slideOutVertically(tween(ID_BAR_SLIDE_MILLIS)) { -slidePx } + fadeOut(tween(ID_BAR_SLIDE_MILLIS)),
        ) {
            CompositionLocalProvider(LocalIdBarPlateBackground provides plateBackground) {
                Box(modifier = Modifier.padding(bottom = PolkadotTheme.spacings.extraTiny)) {
                    bar()
                }
            }
        }
    }
}

@Composable
private fun IdBarScrim(backdrop: DockletBackdrop?, blurred: Boolean) {
    val colour = PolkadotTheme.colors.bg.surface.main
    val brush = remember(colour) {
        Brush.verticalGradient(
            0f to colour.copy(alpha = SCRIM_TOP_ALPHA),
            SCRIM_MIDDLE_STOP to colour.copy(alpha = SCRIM_MIDDLE_ALPHA),
            1f to colour.copy(alpha = 0f),
        )
    }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .dockletBackdrop(backdrop, RectangleShape) { if (blurred) ScrimBlurSigma else 0.dp }
            .background(brush)
            .statusBarsPadding()
            .height(ChainHealthBarDefaults.ContentHeight)
    )
}
