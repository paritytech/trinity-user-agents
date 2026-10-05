package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.lerp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ShieldOutlined
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.roundToInt
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * The primary operating-mode control of the payment system: how much of what the user receives is held back
 * to gain privacy before it can be spent again.
 *
 * The selected mode is marked by a ring on the track, moved either by tapping a mode or by dragging it, in
 * which case it settles on the nearest one on release. The track carries intermediate units between the three
 * presets: the underlying model is continuous in two parameters, so the positions between presets are the
 * ones a later release opens up.
 *
 * Quota is deliberately absent from the copy — it is managed automatically and the user cannot act on it.
 */
@Composable
fun PaymentPrivacyModeSelector(
    modifier: Modifier = Modifier,
    shape: Shape,
    selectedMode: RecyclingStrategyType,
    onModeSelected: (RecyclingStrategyType) -> Unit
) {
    PolkadotSurface(
        modifier = modifier,
        shape = shape,
        color = PolkadotTheme.colors.bg.surface.container
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased)
        ) {
            Header()

            VerticalSpacer { extraMedium }

            ModeSelector(
                selectedMode = selectedMode,
                onModeSelected = onModeSelected
            )
        }
    }
}

@Composable
private fun Header() {
    Row(verticalAlignment = Alignment.CenterVertically) {
        NovaIcon(
            modifier = Modifier.size(HEADER_ICON_SIZE),
            imageVector = NovaIcons.ShieldOutlined,
            tint = PolkadotTheme.colors.fg.secondary
        )

        HorizontalSpacer { small }

        NovaText(
            text = stringResource(RCommon.string.payment_privacy_mode_title),
            style = PolkadotTheme.typography.title.small,
            color = PolkadotTheme.colors.fg.primary
        )
    }
}

@Composable
private fun ModeSelector(
    selectedMode: RecyclingStrategyType,
    onModeSelected: (RecyclingStrategyType) -> Unit
) {
    val modes = RecyclingStrategyType.entries
    val appearances = modes.map { it.appearance() }.toImmutableList()
    val selectedIndex = modes.indexOf(selectedMode)
    val currentSelectedIndex by rememberUpdatedState(selectedIndex)
    val currentOnModeSelected by rememberUpdatedState(onModeSelected)

    val scope = rememberCoroutineScope()
    // Position is held as a fractional mode index rather than pixels, so it is already correct on the first
    // frame — before the row has been measured and a pixel anchor could be known.
    val position = remember { Animatable(selectedIndex.toFloat()) }
    var isDragging by remember { mutableStateOf(false) }
    var trackWidth by remember { mutableIntStateOf(0) }

    val interactionSources = remember { List(modes.size) { MutableInteractionSource() }.toImmutableList() }

    // How long a crossing's cross-fades run. Written on every pointer sample and read only when a
    // crossing starts one, so it is deliberately kept out of the snapshot system: an observable value here
    // would recompose the selector on every sample of every drag without changing anything on screen.
    val dragFade = remember { DragFade() }

    val haptics = LocalHapticFeedback.current
    // The mark the ring was last over. Held across drag events so a tick fires on crossing one,
    // not on every pointer sample.
    var lastMarkIndex by remember { mutableIntStateOf(0) }

    val nearestIndex by remember {
        derivedStateOf { position.value.roundToInt().coerceIn(modes.indices) }
    }

    // Outside a drag the selected mode is the truth; only while a finger is down does the nearest one lead,
    // so the circles and the description follow the ring rather than the mode still committed.
    val highlightedIndex = if (isDragging) nearestIndex else selectedIndex

    // A zero-distance tween still runs its full length and the glow waits on it, so the ring is only animated
    // home when it is away.
    LaunchedEffect(selectedIndex, isDragging) {
        if (!isDragging && position.value != selectedIndex.toFloat()) {
            position.animateTo(selectedIndex.toFloat(), SELECTION_ANIMATION)
        }
    }

    Column {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(MODE_BOX_SIZE)
                .onSizeChanged { trackWidth = it.width }
        ) {
            // The visuals carry no semantics of their own; the touch targets below describe each mode in one
            // node, so a screen reader reads a mode once rather than as three loose fragments.
            Box(modifier = Modifier.clearAndSetSemantics { }) {
                PrivacyModeTrack(
                    modifier = Modifier
                        .align(Alignment.Center)
                        .fillMaxWidth()
                        .height(TRACK_HEIGHT)
                )

                modes.indices.forEach { index ->
                    ModeCircle(
                        modifier = Modifier
                            .align(Alignment.CenterStart)
                            .offset(centreOffset({ index.toFloat() }, { trackWidth }, modes.lastIndex)),
                        appearance = appearances[index],
                        state = when {
                            !isDragging && !position.isRunning && index == selectedIndex -> CircleState.Settled
                            index == highlightedIndex -> CircleState.Grown
                            else -> CircleState.Resting
                        },
                        interactionSource = interactionSources[index]
                    )
                }

                SelectionRing(
                    modifier = Modifier
                        .align(Alignment.CenterStart)
                        .offset(centreOffset({ position.value }, { trackWidth }, modes.lastIndex)),
                    colors = appearances[nearestIndex].colors.selected,
                    fadeMillis = { if (isDragging) dragFade.millis else TAP_FADE_MILLIS }
                )
            }

            // The drag lives on the parent of the touch targets: a tap never crosses the slop, so it reaches
            // the cell below, and once a drag starts the cell's own click is cancelled.
            Row(
                modifier = Modifier
                    .matchParentSize()
                    .selectableGroup()
                    .pointerInput(modes.size) {
                        detectHorizontalDragGestures(
                            onDragStart = {
                                isDragging = true
                                dragFade.millis = SLOW_DRAG_FADE_MILLIS
                                lastMarkIndex = markIndexOf(position.value, size.width, TRACK_INSET.toPx(), TICK_STEP.toPx(), modes.lastIndex)
                            },
                            onDragCancel = {
                                scope.launch {
                                    position.animateTo(currentSelectedIndex.toFloat(), SELECTION_ANIMATION)
                                    isDragging = false
                                }
                            },
                            onDragEnd = {
                                val nearest = position.value.roundToInt().coerceIn(modes.indices)
                                if (nearest != currentSelectedIndex) {
                                    haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                                }
                                currentOnModeSelected(modes[nearest])

                                scope.launch {
                                    position.animateTo(nearest.toFloat(), SELECTION_ANIMATION)
                                    isDragging = false
                                }
                            },
                            onHorizontalDrag = { change, dragAmount ->
                                change.consume()

                                val step = trackStep(size.width, TRACK_INSET.toPx(), modes.lastIndex)
                                if (step > 0f) {
                                    val elapsed = change.uptimeMillis - change.previousUptimeMillis
                                    dragFade.observe(dragAmount, step, elapsed)

                                    val dragged = (change.position.x - TRACK_INSET.toPx()) / step
                                    val clamped = dragged.coerceIn(0f, modes.lastIndex.toFloat())

                                    val mark = markIndexOf(
                                        clamped,
                                        size.width,
                                        TRACK_INSET.toPx(),
                                        TICK_STEP.toPx(),
                                        modes.lastIndex
                                    )
                                    if (mark != lastMarkIndex) {
                                        lastMarkIndex = mark
                                        haptics.performHapticFeedback(HapticFeedbackType.SegmentFrequentTick)
                                    }

                                    scope.launch { position.snapTo(clamped) }
                                }
                            }
                        )
                    }
            ) {
                modes.forEachIndexed { index, mode ->
                    ModeTouchTarget(
                        appearance = appearances[index],
                        isSelected = mode == selectedMode,
                        interactionSource = interactionSources[index],
                        onClick = {
                            if (mode != selectedMode) {
                                haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                            }
                            onModeSelected(mode)
                        }
                    )
                }
            }
        }

        VerticalSpacer { small }

        SelectedModeDescription(appearance = appearances[highlightedIndex])
    }
}

// [position] is a fractional mode index, so the selection tracks a finger continuously.
private fun centreOffset(
    position: () -> Float,
    trackWidth: () -> Int,
    lastIndex: Int
): Density.() -> IntOffset = {
    val inset = TRACK_INSET.toPx()
    val centre = inset + position() * trackStep(trackWidth(), inset, lastIndex)

    IntOffset(x = (centre - MODE_BOX_SIZE.toPx() / 2f).roundToInt(), y = 0)
}

private fun trackStep(trackWidth: Int, inset: Float, lastIndex: Int): Float {
    val span = trackWidth - inset * 2f

    return if (span > 0f && lastIndex > 0) span / lastIndex else 0f
}

private fun markIndexOf(
    position: Float,
    trackWidth: Int,
    inset: Float,
    markStep: Float,
    lastIndex: Int
): Int {
    val offsetFromScaleStart = position * trackStep(trackWidth, inset, lastIndex)

    return if (markStep > 0f) floor(offsetFromScaleStart / markStep).toInt() else 0
}

private class DragFade {
    var millis: Int = SLOW_DRAG_FADE_MILLIS

    fun observe(dragAmount: Float, step: Float, elapsedMillis: Long) {
        val speed = abs(dragAmount) / step * MILLIS_IN_SECOND / elapsedMillis.coerceAtLeast(1L)
        val fraction = (speed / FAST_DRAG_SPEED).coerceIn(0f, 1f)

        // A single pointer sample is jittery; the fade should follow the gesture, not one frame of it.
        millis = lerp(millis, lerp(SLOW_DRAG_FADE_MILLIS, FAST_DRAG_FADE_MILLIS, fraction), FADE_SMOOTHING)
    }
}

private const val SLOW_DRAG_FADE_MILLIS = 280
private const val FAST_DRAG_FADE_MILLIS = 140
private const val TAP_FADE_MILLIS = FAST_DRAG_FADE_MILLIS

// Mode widths per second at which the fade reaches its shortest.
private const val FAST_DRAG_SPEED = 3f

private const val FADE_SMOOTHING = 0.4f
private const val MILLIS_IN_SECOND = 1000f

private val HEADER_ICON_SIZE = 24.dp

@Preview
@Composable
private fun PaymentPrivacyModeSelectorPreview() {
    PolkadotTheme {
        Column(
            modifier = Modifier.padding(PolkadotTheme.spacings.mediumIncreased)
        ) {
            RecyclingStrategyType.entries.forEach { mode ->
                PaymentPrivacyModeSelector(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(PolkadotTheme.radii.large),
                    selectedMode = mode,
                    onModeSelected = {}
                )

                VerticalSpacer { mediumIncreased }
            }
        }
    }
}
