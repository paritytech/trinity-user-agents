package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.animation.ValueAnimator
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChangeIgnoreConsumed
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsSlideToConfirmVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.sign

/**
 * Owns the slide: the drag, where it settles, the one confirmation it fires and what a screen
 * reader is told. How the control looks at a given progress is the face's, one per variant.
 */
@Composable
internal fun JsSlideToConfirm(
    widget: JsWidget.SlideToConfirm,
    onConfirm: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val rtl by rememberUpdatedState(LocalLayoutDirection.current == LayoutDirection.Rtl)
    val currentOnConfirm by rememberUpdatedState(onConfirm)
    val endInsetPx = with(LocalDensity.current) { TRAVEL_END_INSET.toPx() }

    val progress = remember { Animatable(0f) }
    var travelPx by remember { mutableFloatStateOf(0f) }
    var dragging by remember { mutableStateOf(false) }
    var heldAtEnd by remember { mutableStateOf(false) }

    // The product settles the control: loading and confirmed pin it at the end, and a tree that
    // is neither puts a control the user slid back at the start.
    LaunchedEffect(widget.loading, widget.confirmed) {
        when {
            widget.loading || widget.confirmed -> {
                heldAtEnd = true
                progress.snapTo(1f)
            }
            heldAtEnd -> {
                heldAtEnd = false
                progress.animateTo(0f, SpringBack)
            }
        }
    }

    val interactive by rememberUpdatedState(
        widget.enabled && !widget.loading && !widget.confirmed && !heldAtEnd,
    )
    val confirm: () -> Unit = {
        if (!heldAtEnd) {
            heldAtEnd = true
            haptics.performHapticFeedback(HapticFeedbackType.Confirm)
            currentOnConfirm()
            scope.launch { progress.animateTo(1f, Settle) }
        }
    }
    val currentConfirm by rememberUpdatedState(confirm)

    Box(
        modifier = modifier
            .fillMaxWidth()
            .graphicsLayer { alpha = if (widget.enabled) 1f else DISABLED_ALPHA }
            .clearAndSetSemantics {
                contentDescription = listOfNotNull(
                    if (widget.confirmed) widget.confirmedText ?: widget.text else widget.text,
                    widget.secondaryText,
                ).joinToString(", ")
                role = Role.Button
                if (widget.loading) progressBarRangeInfo = ProgressBarRangeInfo.Indeterminate
                if (interactive) {
                    onClick(label = widget.text) {
                        currentConfirm()
                        true
                    }
                } else {
                    disabled()
                }
            }
            .onSizeChanged { travelPx = (it.width - endInsetPx).coerceAtLeast(0f) }
            // The loop reads every move itself, consumed or not, and consumes what it uses. A drag
            // that starts on the control is then the control's alone: the bubble's swipe to reply
            // and the feed's scroll see consumed moves and stand down.
            .pointerInput(Unit) {
                val slop = viewConfiguration.touchSlop
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    if (!interactive || travelPx <= 0f) return@awaitEachGesture
                    var slid = progress.value
                    fun follow(dx: Float) {
                        slid = (slid + (if (rtl) -dx else dx) / travelPx).coerceIn(0f, 1f)
                        val target = slid
                        scope.launch(start = CoroutineStart.UNDISPATCHED) { progress.snapTo(target) }
                    }
                    var dx = 0f
                    var dy = 0f
                    var claimed = false
                    var lifted = false
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id }
                                ?: break
                            if (change.changedToUpIgnoreConsumed()) {
                                lifted = true
                                if (claimed) change.consume()
                                break
                            }
                            val delta = change.positionChangeIgnoreConsumed()
                            if (claimed) {
                                follow(delta.x)
                                change.consume()
                                continue
                            }
                            dx += delta.x
                            dy += delta.y
                            if (abs(dx) > slop && abs(dx) >= abs(dy)) {
                                claimed = true
                                dragging = true
                                follow(dx - sign(dx) * slop)
                                change.consume()
                            } else if (abs(dy) > slop) {
                                break
                            }
                        }
                    } finally {
                        dragging = false
                        if (claimed) {
                            if (lifted && slid >= CONFIRM_THRESHOLD) {
                                currentConfirm()
                            } else if (!heldAtEnd) {
                                scope.launch { progress.animateTo(0f, SpringBack) }
                            }
                        }
                    }
                }
            },
    ) {
        when (widget.variant) {
            JsSlideToConfirmVariant.THUMB -> SlideToConfirmThumbFace(
                widget = widget,
                progress = { progress.value },
                travelPx = { travelPx },
                hinting = interactive && !dragging,
            )
            JsSlideToConfirmVariant.REVEAL -> SlideToConfirmRevealFace(
                widget = widget,
                progress = { progress.value },
            )
        }
    }
}

/** A pill track, a thumb that travels it, and a label the thumb fades as it passes. */
@Composable
private fun SlideToConfirmThumbFace(
    widget: JsWidget.SlideToConfirm,
    progress: () -> Float,
    travelPx: () -> Float,
    hinting: Boolean,
    modifier: Modifier = Modifier,
) {
    val colors = PolkadotTheme.colors
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    val shimmerSweep = if (hinting && ValueAnimator.areAnimatorsEnabled()) {
        rememberInfiniteTransition(label = "slideToConfirmShimmer").animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(SHIMMER_SWEEP_MILLIS, easing = LinearEasing)),
            label = "slideToConfirmShimmerSweep",
        )
    } else {
        null
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(TRACK_HEIGHT)
            .clip(CircleShape)
            .background(colors.bg.surface.nested)
            .padding(PolkadotTheme.spacings.small),
        contentAlignment = Alignment.CenterStart,
    ) {
        Column(
            modifier = Modifier
                // A confirmed control has its thumb at the end, so the label takes the start.
                .padding(
                    start = if (widget.confirmed) PolkadotTheme.spacings.medium else THUMB_SIZE + PolkadotTheme.spacings.extraMedium,
                    end = if (widget.confirmed) THUMB_SIZE + PolkadotTheme.spacings.extraMedium else 0.dp,
                )
                .graphicsLayer {
                    alpha = if (widget.confirmed) 1f else (1f - progress() / LABEL_GONE_AT_FRACTION).coerceIn(0f, 1f)
                },
        ) {
            Text(
                text = if (widget.confirmed) widget.confirmedText ?: widget.text else widget.text,
                style = PolkadotTheme.typography.body.largeEmphasized,
                color = colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.idleShimmer(shimmerSweep, colors.fg.staticWhite.copy(alpha = SHIMMER_ALPHA)),
            )
            widget.secondaryText?.let { secondary ->
                Text(
                    text = secondary,
                    style = PolkadotTheme.typography.body.large,
                    color = colors.fg.tertiary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }

        val trackPaddingPx = with(LocalDensity.current) { PolkadotTheme.spacings.small.toPx() }
        val thumbTop = widget.accent?.toComposeColor() ?: colors.fg.primary
        val thumbBottom = lerp(thumbTop, colors.fg.secondary, THUMB_GRADIENT_DEPTH)
        Box(
            modifier = Modifier
                .offset {
                    // The track's padding is inside the travel the owner measured.
                    IntOffset((progress() * (travelPx() - 2 * trackPaddingPx)).coerceAtLeast(0f).roundToInt(), 0)
                }
                .size(THUMB_SIZE)
                .clip(CircleShape)
                .background(Brush.verticalGradient(listOf(thumbTop, thumbBottom))),
            contentAlignment = Alignment.Center,
        ) {
            when {
                widget.confirmed -> NovaIcon(
                    imageVector = NovaIcons.Check,
                    modifier = Modifier.size(CHEVRON_SIZE),
                    tint = colors.fg.primaryInverted,
                )
                else -> NovaIcon(
                    imageVector = NovaIcons.ArrowRight,
                    modifier = Modifier
                        .size(CHEVRON_SIZE)
                        .graphicsLayer { scaleX = if (rtl) -1f else 1f },
                    tint = colors.fg.primaryInverted,
                )
            }
        }
    }
}

private fun Modifier.idleShimmer(sweep: State<Float>?, highlight: Color): Modifier {
    if (sweep == null) return this
    return graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
        .drawWithContent {
            drawContent()
            val band = size.width * SHIMMER_BAND_FRACTION
            val start = -band + (size.width + band) * sweep.value
            drawRect(
                brush = Brush.horizontalGradient(
                    colors = listOf(Color.Transparent, highlight, Color.Transparent),
                    startX = start,
                    endX = start + band,
                ),
                blendMode = BlendMode.SrcAtop,
            )
        }
}

private val TRACK_HEIGHT = 72.dp
private val THUMB_SIZE = 56.dp
private val CHEVRON_SIZE = 24.dp

// What a full slide leaves uncovered at the end: a thumb's width, or the last of a peeled face.
private val TRAVEL_END_INSET = 56.dp
private const val DISABLED_ALPHA = 0.4f
private const val CONFIRM_THRESHOLD = 0.9f

private const val LABEL_GONE_AT_FRACTION = 0.6f
private const val THUMB_GRADIENT_DEPTH = 0.3f
private const val SHIMMER_SWEEP_MILLIS = 2200
private const val SHIMMER_BAND_FRACTION = 0.4f
private const val SHIMMER_ALPHA = 0.7f

private val SpringBack = spring<Float>(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMediumLow)
private val Settle = tween<Float>(durationMillis = 160)
