package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.animation.ValueAnimator
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChangeIgnoreConsumed
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePhase
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanRelease
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import kotlin.math.roundToInt
import kotlin.math.sign

/**
 * Draws the product's children and reports taps and pans on them; it has no face of its own.
 *
 * Without `pan.drag` the host only reports: start, every move (throttled by `moveIntervalMs`) and the end, and the
 * product moves the children with new trees. With `pan.drag` the host moves the children itself, with an offset that
 * lives here and so survives new trees while the node keeps its place, and the product hears start and end only.
 *
 * The recogniser sits on the node, which never moves, and the children are offset inside it, as #2099's slide to
 * confirm does with its track and thumb. The travel is taken when a pan starts, so a product that moves the children
 * with a margin does not shrink it mid-drag. Event places are in dp from the travel's start.
 */
@Composable
internal fun JsGesture(
    widget: JsWidget.Gesture,
    onAction: (actionId: String, event: JsUiEvent.Type.Gesture) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val density = LocalDensity.current.density
    val rtl by rememberUpdatedState(LocalLayoutDirection.current == LayoutDirection.Rtl)
    val current by rememberUpdatedState(widget)
    val currentOnAction by rememberUpdatedState(onAction)

    // Host-owned place of the children, in px from the travel's start. Kept across trees at the same place.
    val placeX = remember { Animatable(0f) }
    val placeY = remember { Animatable(0f) }
    var nodeWidth by remember { mutableIntStateOf(0) }
    var childrenWidth by remember { mutableIntStateOf(0) }
    var heldAtEnd by remember { mutableStateOf(false) }

    fun travelNow(): Float = GestureRules.travelPx(current.pan?.drag?.travel, nodeWidth, childrenWidth, density)

    fun send(actionId: String?, phase: JsGesturePhase, x: Float, y: Float, travel: Float) {
        actionId ?: return
        val axis = current.pan?.axis ?: JsPanAxis.HORIZONTAL
        val along = if (axis == JsPanAxis.VERTICAL) y else x
        currentOnAction(
            actionId,
            JsUiEvent.Type.Gesture(
                phase = phase,
                x = (x / density).roundToInt(),
                y = (y / density).roundToInt(),
                fractionPermille = GestureRules.permille(along, travel),
            ),
        )
    }

    fun moveTo(x: Float, y: Float, animated: Boolean) {
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            if (animated && ValueAnimator.areAnimatorsEnabled()) {
                launch { placeX.animateTo(x, SpringBack) }
                placeY.animateTo(y, SpringBack)
            } else {
                placeX.snapTo(x)
                placeY.snapTo(y)
            }
        }
    }

    /** A completed drag: to the end, the one success, and held there unless the product asked otherwise. */
    fun complete(travel: Float) {
        val pan = current.pan ?: return
        val drag = pan.drag ?: return
        val endX = if (pan.axis == JsPanAxis.VERTICAL) 0f else travel
        val endY = if (pan.axis == JsPanAxis.VERTICAL) travel else 0f
        heldAtEnd = drag.holdAtEnd
        if (current.hapticOnComplete) haptics.performHapticFeedback(HapticFeedbackType.Confirm)
        send(pan.endAction, JsGesturePhase.COMPLETED, endX, endY, travel)
        if (drag.holdAtEnd) moveTo(endX, endY, animated = true) else moveTo(0f, 0f, animated = true)
    }

    // A tree that turns the node back on frees a held drag; one that drops the drag puts the children back.
    LaunchedEffect(widget.enabled, widget.pan?.drag == null) {
        if (widget.pan?.drag == null || widget.enabled && heldAtEnd) {
            heldAtEnd = false
            placeX.snapTo(0f)
            placeY.snapTo(0f)
        }
    }

    val label = widget.accessibilityLabel
    val accessibleComplete = widget.pan?.drag?.release is JsPanRelease.Complete
    val accessible = GestureRules.accepts(widget.enabled, heldAtEnd) && (accessibleComplete || widget.tapAction != null)

    Box(
        modifier = modifier
            .onSizeChanged { nodeWidth = it.width }
            .semantics {
                label?.let { contentDescription = it }
                role = Role.Button
                if (accessible) {
                    onClick(label = label) {
                        // The accessibility action sends what a successful release sends.
                        val travel = travelNow()
                        if (accessibleComplete) {
                            if (travel > 0f) complete(travel)
                        } else {
                            send(current.tapAction, JsGesturePhase.TAP, 0f, 0f, travel)
                        }
                        true
                    }
                } else {
                    disabled()
                }
            }
            // #2099's loop: it reads every move itself, consumed or not, and consumes what it uses, so a drag that
            // starts on the node is the node's alone and swipe to reply and the feed's scroll stand down.
            .pointerInput(Unit) {
                val slop = viewConfiguration.touchSlop
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val node = current
                    if (!GestureRules.accepts(node.enabled, heldAtEnd)) return@awaitEachGesture
                    val pan = node.pan
                    if (pan == null && node.tapAction == null) return@awaitEachGesture
                    val drag = pan?.drag
                    val travel = travelNow()
                    // Without a drag the product owns the place, so every pan starts from zero, as on iOS.
                    val originX = if (drag != null) placeX.value else 0f
                    val originY = if (drag != null) placeY.value else 0f
                    var x = originX
                    var y = originY
                    var lastMoveAt: Long? = null

                    fun track(dx: Float, dy: Float, at: Long) {
                        pan ?: return
                        val along = if (rtl) -dx else dx
                        when (pan.axis) {
                            JsPanAxis.HORIZONTAL -> x = GestureRules.place(x, along, travel)
                            JsPanAxis.VERTICAL -> y = GestureRules.place(y, dy, travel)
                            JsPanAxis.FREE -> {
                                x = GestureRules.place(x, along, travel)
                                y = GestureRules.place(y, dy, travel)
                            }
                        }
                        if (drag != null) moveTo(x, y, animated = false)
                        if (pan.moveAction != null && GestureRules.movesNow(at, lastMoveAt, pan.moveIntervalMs)) {
                            lastMoveAt = at
                            send(pan.moveAction, JsGesturePhase.MOVED, x, y, travel)
                        }
                    }

                    var total = Offset.Zero
                    var claimed = false
                    var lifted = false
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (change.changedToUpIgnoreConsumed()) {
                                lifted = true
                                if (claimed) change.consume()
                                break
                            }
                            val delta = change.positionChangeIgnoreConsumed()
                            if (claimed) {
                                track(delta.x, delta.y, change.uptimeMillis)
                                change.consume()
                                continue
                            }
                            total += delta
                            if (pan == null) {
                                if (total.getDistance() > slop) break else continue
                            }
                            when (GestureRules.decide(pan.axis, total.x, total.y, slop)) {
                                SlideDragDecision.CLAIM -> {
                                    claimed = true
                                    send(pan.startAction, JsGesturePhase.STARTED, x, y, travel)
                                    // From where the claim was made, so the children don't jump by the slop.
                                    when (pan.axis) {
                                        JsPanAxis.HORIZONTAL -> track(total.x - sign(total.x) * slop, 0f, change.uptimeMillis)
                                        JsPanAxis.VERTICAL -> track(0f, total.y - sign(total.y) * slop, change.uptimeMillis)
                                        JsPanAxis.FREE -> track(total.x, total.y, change.uptimeMillis)
                                    }
                                    change.consume()
                                }
                                SlideDragDecision.RELEASE -> break
                                SlideDragDecision.WAIT -> Unit
                            }
                        }
                    } finally {
                        when {
                            claimed && pan != null -> {
                                val settle = when {
                                    !lifted -> null
                                    drag == null -> GestureSettle.STAY
                                    else -> GestureRules.settle(drag.release, GestureRules.permille(if (pan.axis == JsPanAxis.VERTICAL) y else x, travel), travel)
                                }
                                when (settle) {
                                    // Taken away mid-drag: never a completion, and the children go back where they were.
                                    null -> {
                                        send(pan.endAction, JsGesturePhase.CANCELLED, x, y, travel)
                                        if (drag != null) moveTo(originX, originY, animated = true)
                                    }
                                    GestureSettle.COMPLETE -> complete(travel)
                                    GestureSettle.SNAP_BACK -> {
                                        send(pan.endAction, JsGesturePhase.ENDED, x, y, travel)
                                        moveTo(0f, 0f, animated = true)
                                    }
                                    GestureSettle.STAY -> send(pan.endAction, JsGesturePhase.ENDED, x, y, travel)
                                }
                            }
                            lifted && total.getDistance() <= slop -> send(node.tapAction, JsGesturePhase.TAP, 0f, 0f, travel)
                        }
                    }
                }
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(
            modifier = Modifier
                .onSizeChanged { childrenWidth = it.width }
                .offset {
                    // Clamped here too: a tree that narrows the node mid-hold must not push the children out of it.
                    val travel = travelNow()
                    IntOffset(placeX.value.coerceIn(0f, travel).roundToInt(), placeY.value.coerceIn(0f, travel).roundToInt())
                },
        ) {
            content()
        }
    }
}

private val SpringBack = spring<Float>(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMediumLow)
