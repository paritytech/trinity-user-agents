package io.paritytech.polkadotapp.app.root.presentation.main.compose.components

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.app.root.domain.main.DockletTip
import io.paritytech.polkadotapp.design.components.tooltip.NonFocusablePopupProperties
import io.paritytech.polkadotapp.design.components.tooltip.PolkadotTooltip
import io.paritytech.polkadotapp.design.components.tooltip.PolkadotTooltipContent
import io.paritytech.polkadotapp.design.components.tooltip.TooltipAlignment
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlin.math.hypot
import io.paritytech.polkadotapp.common.R as RCommon

private const val FlingAwayDp = 56f

@Composable
fun DockletTipPopup(
    tip: DockletTip?,
    onRestore: () -> Unit,
    onDismiss: () -> Unit,
) {
    var shownTip by remember { mutableStateOf(tip) }
    if (tip != null) shownTip = tip
    val currentOnRestore by rememberUpdatedState(onRestore)

    PolkadotTooltip(
        expanded = tip != null,
        onDismiss = onDismiss,
        backgroundColor = PolkadotTheme.colors.fg.primary,
        shape = PolkadotTheme.shapes.tiny,
        arrowVisible = true,
        alignment = TooltipAlignment.Start,
        properties = NonFocusablePopupProperties,
    ) {
        Box(
            modifier = Modifier.pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    var moved = 0f
                    while (true) {
                        val change = awaitPointerEvent(PointerEventPass.Initial).changes.firstOrNull { it.id == down.id } ?: break
                        val delta = change.position - down.position
                        moved = maxOf(moved, hypot(delta.x, delta.y).toDp().value)
                        if (!change.pressed) break
                    }
                    if (moved > FlingAwayDp) currentOnRestore()
                }
            },
        ) {
            PolkadotTooltipContent(
                title = stringResource(if (shownTip == DockletTip.AutoHidden) RCommon.string.docklet_auto_hidden_tip_title else RCommon.string.docklet_tucked_tip_title),
                message = stringResource(if (shownTip == DockletTip.AutoHidden) RCommon.string.docklet_auto_hidden_tip_message else RCommon.string.docklet_tucked_tip_message),
                onDismiss = onRestore,
            )
        }
    }
}
