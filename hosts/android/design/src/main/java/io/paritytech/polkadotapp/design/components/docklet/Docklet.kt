package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.clickable
import androidx.compose.foundation.indication
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.lerp
import io.paritytech.polkadotapp.design.components.badge.NumberBadge
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ChatFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.GlobeAltFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.SettingsFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.WalletFilled
import io.paritytech.polkadotapp.design.components.navigationbar.NotificationDot
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.blurEffect
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

private val SlotIconSize = 28.dp
private val HighlightMillis = DockletMotion.HighlightDuration.inWholeMilliseconds.toInt()

@Immutable
data class DockletSlot(
    val icon: ImageVector,
    val contentDescription: String,
    val selected: Boolean,
    val badgeCount: Int,
    val warning: Boolean,
    val onClick: () -> Unit,
)

@Composable
fun Docklet(
    modifier: Modifier,
    slots: ImmutableList<DockletSlot>,
    cardProgress: () -> Float,
    backdrop: DockletBackdrop?,
) {
    val haptics = rememberDockletHaptics()
    Box(modifier = modifier) {
        DockletGlassTheme {
            DockSurface(slots, cardProgress, backdrop) { slot ->
                if (cardProgress() <= 0f) haptics.perform(DockletMoment.SlotTap)
                slot.onClick()
            }
        }
    }
}

@Composable
private fun DockSurface(
    slots: ImmutableList<DockletSlot>,
    cardProgress: () -> Float,
    backdrop: DockletBackdrop?,
    onSlotTap: (DockletSlot) -> Unit,
) {
    val palette = LocalDockletPalette.current
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(DockletGeometry.DockHeight),
    ) {
        Box(
            modifier = Modifier
                .matchParentSize()
                .dockletBackdrop(
                    backdrop = backdrop,
                    shape = DockShape,
                    sigma = { DockletMaterials.DockBlurSigma },
                )
                .drawWithCache {
                    val path = dockPath(size, density)
                    onDrawBehind { drawDockletGlass(path, palette) }
                }
                .clip(DockShape),
        ) {
            SlotRow(
                modifier = Modifier
                    .padding(vertical = DockletGeometry.ItemRowPadding)
                    .fillMaxWidth()
                    .height(DockletGeometry.ItemRowHeight)
                    .graphicsLayer {
                        val dim = cardProgress().coerceIn(0f, 1f)
                        alpha = lerp(1f, DockletMaterials.DockItemsFadedAlpha, dim)
                        renderEffect = blurEffect(DockletMaterials.DockItemsBlurSigma * dim, density)
                    },
                slots = slots,
                onSlotTap = onSlotTap,
            )
        }
    }
}

@Composable
private fun SlotRow(
    modifier: Modifier,
    slots: ImmutableList<DockletSlot>,
    onSlotTap: (DockletSlot) -> Unit,
) {
    BoxWithConstraints(modifier = modifier) {
        val centres = DockletGeometry.slotCentres(maxWidth)
        val spans = DockletGeometry.touchSpans(maxWidth)
        val selectedIndex = slots.indexOfFirst { it.selected }
        val highlightCentre by animateDpAsState(
            targetValue = centres.getOrElse(selectedIndex) { centres.first() },
            animationSpec = tween(HighlightMillis, easing = DockletMotion.HighlightEasing),
            label = "DockletHighlightCentre",
        )
        if (selectedIndex in centres.indices) {
            PolkadotSurface(
                modifier = Modifier
                    .offset { IntOffset((highlightCentre - DockletGeometry.SlotWidth / 2).roundToPx(), 0) }
                    .size(DockletGeometry.SlotWidth, DockletGeometry.ItemRowHeight),
                shape = CircleShape,
                color = LocalDockletPalette.current.highlight,
            ) {}
        }
        slots.zip(spans).forEachIndexed { index, (slot, span) ->
            SlotItem(
                span = span,
                slot = slot,
                iconShift = centres[index] - (span.start + span.end) / 2,
                onClick = { onSlotTap(slot) },
            )
        }
    }
}

@Composable
private fun SlotItem(span: DockletSpan, slot: DockletSlot, iconShift: Dp, onClick: () -> Unit) {
    val interactionSource = remember { MutableInteractionSource() }
    val selection by animateFloatAsState(
        targetValue = if (slot.selected) 1f else 0f,
        animationSpec = tween(HighlightMillis, easing = DockletMotion.HighlightEasing),
        label = "DockletSlotSelection",
    )
    val iconColor = lerp(PolkadotTheme.colors.fg.secondary, PolkadotTheme.colors.fg.primary, selection)
    Box(
        modifier = Modifier
            .offset { IntOffset(span.start.roundToPx(), 0) }
            .width(span.width)
            .fillMaxHeight()
            .semantics { contentDescription = slot.contentDescription }
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                role = Role.Tab,
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier
                .offset { IntOffset(iconShift.roundToPx(), 0) }
                .requiredSize(DockletGeometry.SlotWidth, DockletGeometry.ItemRowHeight)
                .clip(CircleShape)
                .indication(interactionSource, LocalIndication.current),
        )
        Box(modifier = Modifier.offset { IntOffset(iconShift.roundToPx(), 0) }) {
            Box(
                modifier = Modifier
                    .padding(horizontal = PolkadotTheme.spacings.small)
                    .size(SlotIconSize),
            ) {
                NovaIcon(
                    modifier = Modifier.fillMaxSize(),
                    imageVector = slot.icon,
                    tint = iconColor,
                )
            }
            NotificationDot(modifier = Modifier.align(Alignment.TopEnd), visible = slot.warning)
            if (slot.badgeCount > 0) {
                NumberBadge(modifier = Modifier.align(Alignment.TopEnd), number = slot.badgeCount)
            }
        }
    }
}

@Preview(widthDp = 392)
@Composable
private fun DockletPreview() {
    PolkadotTheme {
        Docklet(
            modifier = Modifier
                .padding(DockletGeometry.ItemRowPadding)
                .width(372.dp),
            slots = persistentListOf(
                DockletSlot(NovaIcons.ChatFilled, "", selected = true, badgeCount = 0, warning = false) {},
                DockletSlot(NovaIcons.WalletFilled, "", selected = false, badgeCount = 0, warning = true) {},
                DockletSlot(NovaIcons.GlobeAltFilled, "", selected = false, badgeCount = 3, warning = false) {},
                DockletSlot(NovaIcons.SettingsFilled, "", selected = false, badgeCount = 0, warning = false) {},
            ),
            cardProgress = { 0f },
            backdrop = null,
        )
    }
}
