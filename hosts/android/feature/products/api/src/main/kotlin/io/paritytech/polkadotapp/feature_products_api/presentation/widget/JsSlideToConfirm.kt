package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.animation.ValueAnimator
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
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
import io.paritytech.polkadotapp.feature_products_api.model.JsSlideToConfirmVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
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
    val density = LocalDensity.current
    // Whole pixels, as the layout places the thumb and the track's padding.
    val endInsetPx by rememberUpdatedState(with(density) { TRAVEL_END_INSET.roundToPx() }.toFloat())
    val sideInsetPx by rememberUpdatedState(with(density) { SlideToConfirmRules.travelSideInsetDp(widget).dp.roundToPx() }.toFloat())

    val progress = remember { Animatable(0f) }
    var widthPx by remember { mutableIntStateOf(0) }
    // One travel for the drag and the face, read where it is used: the thumb follows the finger one for one.
    val travelPx = { SlideToConfirmRules.travelPx(widthPx.toFloat(), endInsetPx, sideInsetPx) }
    var dragging by remember { mutableStateOf(false) }
    var heldAtEnd by remember { mutableStateOf(false) }
    // The release's spring home may dip below the start, as the existing slider's does; the newest release that started it.
    var releaseNumber by remember { mutableIntStateOf(0) }
    var dippingRelease by remember { mutableIntStateOf(-1) }
    // The existing slider's feel (SlideStyle.feel): the thumb grown while pressed, and its pop after a confirmation.
    val feel = SlideStyle.feel
    val pressScale = remember { Animatable(1f) }
    var pressJob by remember { mutableStateOf<Job?>(null) }
    var popped by remember { mutableStateOf<Pair<Float, Float>?>(null) }
    var popJob by remember { mutableStateOf<Job?>(null) }
    var heightPx by remember { mutableIntStateOf(0) }
    val animates = { ValueAnimator.areAnimatorsEnabled() }

    fun press() {
        pressJob?.cancel()
        pressJob = scope.launch { pressScale.animateTo(feel.pressScale, tween(feel.pressMs, easing = WidgetStyleRules.EaseOutQuad)) }
    }

    fun unpress() {
        if (pressScale.value == 1f && pressScale.targetValue == 1f) return
        pressJob?.cancel()
        pressJob = scope.launch { pressScale.animateTo(1f, tween(feel.unpressMs, easing = WidgetStyleRules.EaseOutQuad)) }
    }

    /** The existing slider's finish: the thumb eases out to the end (117 ms, longer over a longer way) and pops as it arrives. */
    fun finish() {
        if (!animates()) {
            scope.launch { progress.snapTo(1f) }
            scope.launch { pressScale.snapTo(1f) }
            return
        }
        val approach = WidgetStyleRules.approachMs(feel, (1f - progress.value) * travelPx() / density.density)
        scope.launch { progress.animateTo(1f, tween(approach, easing = WidgetStyleRules.EaseOutQuad)) }
        pressJob?.cancel()
        popJob?.cancel()
        val from = pressScale.value
        popJob = scope.launch {
            try {
                val start = withFrameNanos { it } + (approach - feel.approachMs) * NANOS_PER_MS
                while (true) {
                    val now = withFrameNanos { it }
                    popped = WidgetStyleRules.pop(feel, (now - start) / NANOS_PER_FRAME, from) ?: break
                }
            } finally {
                popped = null
                scope.launch { pressScale.snapTo(1f) }
            }
        }
    }

    /** A finger down within 10 dp of the thumb, as the existing slider takes it. */
    fun hitsThumb(at: Offset): Boolean {
        val inset = with(density) { SlideToConfirmRules.thumbInsetDp(widget).dp.toPx() }
        val radius = with(density) { THUMB_SIZE.toPx() } / 2f
        val fromStart = inset + radius + progress.value * travelPx()
        val centerX = if (rtl) widthPx - fromStart else fromStart
        val dx = at.x - centerX
        val dy = at.y - heightPx / 2f
        val reach = radius + with(density) { PRESS_REACH.toPx() }
        return dx * dx + dy * dy <= reach * reach
    }

    // The product settles the control: loading and confirmed pin it at the end, and a tree that
    // is neither puts a control the user slid back at the start.
    LaunchedEffect(widget.loading, widget.confirmed) {
        when (SlideToConfirmRules.settle(heldAtEnd, widget.loading, widget.confirmed)) {
            SlideSettle.PIN_AT_END -> {
                heldAtEnd = true
                progress.snapTo(1f)
            }
            SlideSettle.RETURN_TO_START -> {
                heldAtEnd = false
                progress.animateTo(0f, SliderSpringBack)
            }
            SlideSettle.STAY -> Unit
        }
    }

    val interactive by rememberUpdatedState(SlideToConfirmRules.acceptsSlide(widget, heldAtEnd))
    val confirm: () -> Unit = {
        if (!heldAtEnd) {
            heldAtEnd = true
            haptics.performHapticFeedback(HapticFeedbackType.Confirm)
            currentOnConfirm()
            finish()
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
            .onSizeChanged {
                widthPx = it.width
                heightPx = it.height
            }
            // The loop reads every move itself, consumed or not, and consumes what it uses. A drag
            // that starts on the control is then the control's alone: the bubble's swipe to reply
            // and the feed's scroll see consumed moves and stand down.
            .pointerInput(Unit) {
                val slop = viewConfiguration.touchSlop
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    if (!interactive || travelPx() <= 0f) return@awaitEachGesture
                    if (widget.variant == JsSlideToConfirmVariant.THUMB && animates() && popJob?.isActive != true &&
                        hitsThumb(down.position)
                    ) {
                        press()
                    }
                    dippingRelease = -1
                    var slid = progress.value.coerceAtLeast(0f)
                    fun follow(dx: Float) {
                        slid = SlideToConfirmRules.slideBy(slid, dx, travelPx(), rtl)
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
                            when (SlideToConfirmRules.decide(dx, dy, slop)) {
                                SlideDragDecision.CLAIM -> {
                                    claimed = true
                                    dragging = true
                                    follow(dx - sign(dx) * slop)
                                    change.consume()
                                }
                                SlideDragDecision.RELEASE -> break
                                SlideDragDecision.WAIT -> Unit
                            }
                        }
                    } finally {
                        dragging = false
                        if (claimed && SlideToConfirmRules.completes(slid, lifted)) {
                            currentConfirm()
                        } else {
                            if (claimed && !heldAtEnd) {
                                val number = ++releaseNumber
                                dippingRelease = number
                                scope.launch {
                                    try {
                                        progress.animateTo(0f, SliderSpringBack)
                                    } finally {
                                        if (dippingRelease == number) dippingRelease = -1
                                    }
                                }
                            }
                            unpress()
                        }
                    }
                }
            },
    ) {
        when (widget.variant) {
            JsSlideToConfirmVariant.THUMB -> SlideToConfirmThumbFace(
                widget = widget,
                progress = { progress.value },
                travelPx = travelPx,
                // The shimmer band runs while the slider waits, is dragged or springs home; not once it is held.
                hinting = interactive,
                heldAtEnd = heldAtEnd || widget.loading || widget.confirmed,
                thumbScale = { popped?.first ?: pressScale.value },
                thumbPast = { popped?.second ?: 0f },
                dipsBelowHome = { dippingRelease == releaseNumber },
            )
            JsSlideToConfirmVariant.REVEAL -> SlideToConfirmRevealFace(
                widget = widget,
                progress = { progress.value },
            )
        }
    }
}

/**
 * A pill track, a thumb that travels it, and a label the thumb fades as it passes, drawn as the existing slider is
 * (SlideStyle): the existing slider's pill clips nothing, so the thumb's shadow and its pop stand over the track's edge.
 */
@Composable
private fun SlideToConfirmThumbFace(
    widget: JsWidget.SlideToConfirm,
    progress: () -> Float,
    travelPx: () -> Float,
    hinting: Boolean,
    heldAtEnd: Boolean,
    thumbScale: () -> Float,
    thumbPast: () -> Float,
    dipsBelowHome: () -> Boolean,
    modifier: Modifier = Modifier,
) {
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    // The track stands as tall as the product asked, as the room footer's 80 does, with the thumb centred in it.
    val trackHeight = SlideToConfirmRules.thumbTrackHeightDp(widget).dp
    val thumbInset = SlideToConfirmRules.thumbInsetDp(widget).dp
    val density = LocalDensity.current.density
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(trackHeight)
            .background(WidgetStyleRules.color(SlideStyle.TRACK), CircleShape)
            .padding(thumbInset),
        contentAlignment = Alignment.CenterStart,
    ) {
        // `#text`: two lines from 84 to 16 short of the pill's ends, centred under a 1 dp padding, 4.6 apart; the existing
        // slider fades them with the thumb's progress, so a control held at the end shows none.
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(
                    start = SlideStyle.TEXT_START_DP.dp - thumbInset,
                    end = SlideStyle.TEXT_END_DP.dp - thumbInset,
                    top = SlideStyle.TEXT_PADDING_TOP_DP.dp,
                )
                .graphicsLayer { alpha = SlideStyle.textOpacity(if (heldAtEnd) 1f else progress()) },
            verticalArrangement = Arrangement.spacedBy(SlideStyle.GAP_DP.dp),
        ) {
            Text(
                text = if (widget.confirmed) widget.confirmedText ?: widget.text else widget.text,
                style = SlideStyle.line1Font.toTextStyle(),
                maxLines = 1,
                softWrap = false,
                overflow = TextOverflow.Ellipsis,
                modifier = if (hinting) SlideStyle.shimmer.toShimmerModifier() else Modifier,
            )
            widget.secondaryText?.takeIf { it.isNotEmpty() }?.let { secondary ->
                Text(
                    text = secondary,
                    style = SlideStyle.line2Font.toTextStyle(),
                    maxLines = 1,
                    softWrap = false,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }

        // `#thumb`: 56, round, `--thumb` (or the product's accent) with its shadow; the chevron, or the spinner while
        // the product works.
        val fill = widget.accent?.toComposeColor() ?: WidgetStyleRules.color(SlideStyle.THUMB)
        val tint = SlideStyle.glyphTint(fill)
        Box(
            modifier = Modifier
                .offset {
                    // The owner's travel: the lane inside the track's padding less the thumb, the distance the drag
                    // divides by. Only the release's spring home may dip below the start, as the existing slider's does.
                    val past = thumbPast() * density * if (rtl) -1f else 1f
                    val along = WorkerThumbSettle.along(progress() * travelPx(), travelPx(), dipsBelowHome())
                    IntOffset((along + past).roundToInt(), 0)
                }
                .size(THUMB_SIZE)
                .graphicsLayer {
                    scaleX = thumbScale()
                    scaleY = thumbScale()
                }
                .then(SlideStyle.shadow.toShadowModifier(CircleShape))
                .background(fill, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (widget.loading) {
                SlideSpinner(tint = tint)
            } else {
                SlideChevron(tint = tint, mirrored = rtl)
            }
        }
    }
}

private val THUMB_SIZE = SlideToConfirmRules.THUMB_SIZE_DP.dp
private val PRESS_REACH = 10.dp
private const val NANOS_PER_MS = 1_000_000L
private const val NANOS_PER_FRAME = 1_000_000_000f / 60f

// What a full slide leaves uncovered at the end: the thumb's width, or the last of a peeled face.
private val TRAVEL_END_INSET = 56.dp
private const val DISABLED_ALPHA = 0.4f

/** The existing slider's damped spring home (18 rad/s, 0.8): the natural frequency squared is the stiffness. */
private val SliderSpringBack = spring<Float>(
    dampingRatio = SlideStyle.feel.springDamping,
    stiffness = SlideStyle.feel.springOmega * SlideStyle.feel.springOmega,
)
