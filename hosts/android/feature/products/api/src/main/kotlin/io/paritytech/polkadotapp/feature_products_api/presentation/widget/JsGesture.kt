package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.animation.ValueAnimator
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.collectAsState
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
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChangeIgnoreConsumed
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePhase
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanFeel
import io.paritytech.polkadotapp.feature_products_api.model.JsPanRelease
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlin.math.roundToInt
import kotlin.math.sign

/**
 * Draws the product's children and reports taps and pans on them; it has no face of its own.
 *
 * Without `pan.drag` the host only reports: start, every move (throttled by `moveIntervalMs`) and the end, and the
 * product moves the children with new trees. A keyed node coalesces moves per frame until its worker answers and
 * draws position-only updates through its own offset state. With `pan.drag` the host moves the children itself, with an offset that
 * lives here and so survives new trees while the node keeps its place, and the product hears start and end only.
 *
 * The recogniser sits on the node, which never moves, and the children are offset inside it, as #2099's slide to
 * confirm does with its track and thumb. The travel is taken when a pan starts, so a product that moves the children
 * with a margin does not shrink it mid-drag. Event places are in dp from the travel's start.
 */
@Composable
internal fun JsGesture(
    modifier: Modifier = Modifier,
    widget: JsWidget.Gesture,
    onAction: (actionId: String, event: JsUiEvent.Type.Gesture) -> Unit,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val view = LocalView.current
    val density = LocalDensity.current.density
    val rtl by rememberUpdatedState(LocalLayoutDirection.current == LayoutDirection.Rtl)
    val current by rememberUpdatedState(widget)
    val currentOnAction by rememberUpdatedState(onAction)
    val workerPosition = widget.position?.value?.collectAsState()
    val moves = remember { GestureMoveCoalescer<Offset>() }

    // Host-owned place of the children, in px from the travel's start. Kept across trees at the same place.
    val placeX = remember { Animatable(0f) }
    val placeY = remember { Animatable(0f) }
    var nodeWidth by remember { mutableIntStateOf(0) }
    var nodeHeight by remember { mutableIntStateOf(0) }
    var childrenWidth by remember { mutableIntStateOf(0) }
    var heldAtEnd by remember { mutableStateOf(false) }
    // An every-move worker thumb's settle after the finger lifts (WorkerThumbSettle), drawn instead of the worker's place.
    val settle = remember { Animatable(Offset.Zero, Offset.VectorConverter) }
    var settling by remember { mutableStateOf(false) }
    var settleJob by remember { mutableStateOf<Job?>(null) }
    // A release home with the existing slider's motion may dip below home, as its spring does (WorkerThumbSettle.onLane):
    // the every-move settle's, and the host drag's spring home (the newest moveTo that asked for it, by its number).
    var settleDips by remember { mutableStateOf(false) }
    var moveNumber by remember { mutableIntStateOf(0) }
    var dippingMove by remember { mutableIntStateOf(-1) }

    // The worker's answer waited for in the frame that sent its move, laid out in that frame (GesturePacingSettings).
    var answeredInFrame by remember { mutableStateOf<Pair<JsGesturePosition, JsGesturePosition.Value>?>(null) }
    val minimumGapNanos = remember(view) { GesturePacingSettings.minimumGapNanos(view.display?.refreshRate) }

    fun travelNow(): Float = GestureRules.travelPx(current.pan?.drag?.travel, nodeWidth, childrenWidth, density)

    // The feel (JsPanFeel): the children grown while pressed, and a completed host drag's pop with how far past the
    // end it stands.
    val pressScale = remember { Animatable(1f) }
    var pressJob by remember { mutableStateOf<Job?>(null) }
    var popped by remember { mutableStateOf<Pair<Float, Float>?>(null) }
    var popJob by remember { mutableStateOf<Job?>(null) }

    /** Where the worker's tree drew the children: the first child's leading margin, in px. */
    fun basePx(): Float = (current.children.firstOrNull()?.modifiers().orEmpty().filterIsInstance<JsModifier.Margin>().firstOrNull()
        ?.let { it.start ?: it.horizontal ?: it.all } ?: 0) * density

    /** Where the worker's place draws the children now, in px, as the offset below lays them out. */
    fun drawnWorkerPx(travel: Float): Offset {
        if (settling) return WorkerThumbSettle.onLane(settle.value, travel, settleDips)
        val position = GestureRules.newest(
            workerPosition?.value,
            answeredInFrame?.takeIf { it.first === current.position }?.second,
        ) ?: return Offset.Zero
        return Offset(position.x / 100f * density, position.y / 100f * density)
    }

    /** The children's place along their whole lane, 0 to 1, for the nodes that fade with this gesture's pan. */
    fun panProgress(): Float {
        val base = basePx()
        val drag = current.pan?.drag
        val lane = if (drag != null) travelNow() else (nodeWidth - childrenWidth).toFloat() + base
        if (lane <= 0f) return 0f
        val vertical = current.pan?.axis == JsPanAxis.VERTICAL
        val worker = drawnWorkerPx(travelNow())
        val host = if (vertical) placeY.value else placeX.value
        return (base + host.coerceIn(0f, travelNow()) + if (vertical) worker.y else worker.x) / lane
    }

    widget.key?.let { key ->
        val place = LocalGesturePanPlaces.current.place(key)
        SideEffect { place.progress = ::panProgress }
    }

    fun press(feel: JsPanFeel) {
        pressJob?.cancel()
        pressJob = scope.launch { pressScale.animateTo(feel.pressScale, tween(feel.pressMs, easing = WidgetStyleRules.EaseOutQuad)) }
    }

    fun unpress() {
        val feel = current.feel ?: return
        if (pressScale.targetValue == 1f && !pressScale.isRunning && pressScale.value == 1f) return
        pressJob?.cancel()
        pressJob = scope.launch { pressScale.animateTo(1f, tween(feel.unpressMs, easing = WidgetStyleRules.EaseOutQuad)) }
    }

    /** A finger down within 10 dp of the round child the tree drew, as the existing slider takes it. */
    fun hitsChild(at: Offset): Boolean {
        val base = basePx()
        val radius = (childrenWidth - base) / 2f
        if (radius <= 0f) return false
        val worker = drawnWorkerPx(travelNow())
        val fromStart = base + placeX.value.coerceIn(0f, travelNow()) + worker.x + radius
        val centerX = if (rtl) nodeWidth - fromStart else fromStart
        val dx = at.x - centerX
        val dy = at.y - nodeHeight / 2f
        val reach = radius + PRESS_REACH_DP * density
        return dx * dx + dy * dy <= reach * reach
    }

    /** The pop after a completed drag: it peaks as the children arrive, from the scale they were pressed to. */
    fun startPop(feel: JsPanFeel, delayMs: Int) {
        pressJob?.cancel()
        popJob?.cancel()
        val from = pressScale.value
        popJob = scope.launch {
            try {
                val start = withFrameNanos { it } + delayMs * NANOS_PER_MS
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

    fun send(actionId: String?, phase: JsGesturePhase, x: Float, y: Float, travel: Float) {
        actionId ?: return
        val axis = current.pan?.axis ?: JsPanAxis.HORIZONTAL
        val along = if (axis == JsPanAxis.VERTICAL) y else x
        currentOnAction(
            actionId,
            JsUiEvent.Type.Gesture(
                phase = phase,
                // Positions travel in hundredths of a dp, so a slow drag still reports every frame's move.
                x = (x / density * 100).roundToInt(),
                y = (y / density * 100).roundToInt(),
                fractionPermille = GestureRules.permille(along, travel),
            ),
        )
    }

    /** A keyed move: sent, then its answer waited for briefly so the frame being made lays it out. */
    fun sendKeyed(position: JsGesturePosition, actionId: String?, point: Offset, travel: Float) {
        actionId ?: return
        val before = position.value.value.answer
        send(actionId, JsGesturePhase.MOVED, point.x, point.y, travel)
        val wait = GesturePacingSettings.currentAnswerWaitNanos
        if (wait <= 0L) return
        val answered = position.awaitAnswer(before, wait)
        val inTime = answered.answer != before
        if (inTime) {
            moves.noteAnswer(answered.answer)
            answeredInFrame = position to answered
        }
    }

    /** Where the worker's thumb is drawn now, in px from the travel's start. */
    fun workerPlacePx(): Offset {
        if (settling) return settle.value
        val shown = GestureRules.newest(
            current.position?.value?.value,
            answeredInFrame?.takeIf { it.first === current.position }?.second,
        ) ?: return Offset.Zero
        return Offset(shown.x / 100f * density, shown.y / 100f * density)
    }

    /** A released every-move thumb settles from where it is drawn instead of jumping to the worker's next place. */
    fun settleWorkerThumb(axis: JsPanAxis, releasedPermille: Int, cancelled: Boolean, travel: Float) {
        if (!WorkerThumbSettle.currentIsOn) return
        val from = workerPlacePx()
        val kind = WorkerThumbSettle.kind(axis, releasedPermille, cancelled)
        val to = WorkerThumbSettle.target(kind, axis, travel)
        if (!WorkerThumbSettle.animates(ValueAnimator.areAnimatorsEnabled(), from, to)) return
        settleJob?.cancel()
        settling = true
        settleDips = kind == WorkerThumbSettle.Kind.HOME && current.feel != null
        settleJob = scope.launch(start = CoroutineStart.UNDISPATCHED) {
            try {
                settle.snapTo(from)
                val spec = settleSpec(kind, current.feel, (to - from).getDistance() / density)
                // With the existing slider's motion, timed from the lift as its own is (FromLift).
                settle.animateTo(to, if (current.feel != null) spec.fromLift() else spec)
            } finally {
                settling = false
            }
        }
    }

    /** A new drag takes the thumb over at once. */
    fun cutSettleShort() {
        settleJob?.cancel()
        settleJob = null
        settling = false
    }

    /** [dips]: a release home with the existing slider's motion, drawn dipping below home as its spring does. */
    fun moveTo(
        x: Float,
        y: Float,
        animated: Boolean,
        spec: AnimationSpec<Float> = current.feel?.springHome() ?: SpringBack,
        dips: Boolean = false,
    ) {
        val number = ++moveNumber
        dippingMove = if (dips) number else -1
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            if (animated && ValueAnimator.areAnimatorsEnabled()) {
                try {
                    coroutineScope {
                        launch { placeX.animateTo(x, spec) }
                        placeY.animateTo(y, spec)
                    }
                } finally {
                    if (dippingMove == number) dippingMove = -1
                }
            } else {
                if (dippingMove == number) dippingMove = -1
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
        val feel = current.feel
        if (feel != null && drag.holdAtEnd && ValueAnimator.areAnimatorsEnabled()) {
            // The existing slider's finish: an ease-out to the end, longer over a longer way, and the pop as it arrives.
            val along = if (pan.axis == JsPanAxis.VERTICAL) placeY.value else placeX.value
            val approach = WidgetStyleRules.approachMs(feel, (travel - along) / density)
            moveTo(endX, endY, animated = true, spec = tween<Float>(approach, easing = WidgetStyleRules.EaseOutQuad).fromLift())
            startPop(feel, approach - feel.approachMs - LIFT_FRAME_MS)
            return
        }
        unpress()
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
            .onSizeChanged {
                nodeWidth = it.width
                nodeHeight = it.height
            }
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
                // Lower than the system slop by default, so the thumb follows after about 1 mm (GestureDragThreshold).
                val slop = GestureDragThreshold.currentPx(viewConfiguration.touchSlop, density)
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val node = current
                    if (!GestureRules.accepts(node.enabled, heldAtEnd)) return@awaitEachGesture
                    val pan = node.pan
                    if (pan == null && node.tapAction == null) return@awaitEachGesture
                    node.feel?.let { feel ->
                        if (ValueAnimator.areAnimatorsEnabled() && popJob?.isActive != true && hitsChild(down.position)) press(feel)
                    }
                    val drag = pan?.drag
                    val travel = travelNow()
                    // Without a drag the product owns the place, so every pan starts from zero, as on iOS.
                    val originX = if (drag != null) placeX.value else 0f
                    val originY = if (drag != null) placeY.value else 0f
                    var x = originX
                    var y = originY
                    var lastMoveAt: Long? = null
                    var moveJob: Job? = null

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
                        if (pan.moveAction != null) {
                            val keyed = node.position
                            if (keyed != null) {
                                moves.offer(Offset(x, y))
                                if (GesturePacingSettings.currentSendOnTouch && current.enabled && current.position === keyed) {
                                    moves.noteAnswer(keyed.value.value.answer)
                                    val gap = maxOf(minimumGapNanos, pan.moveIntervalMs * NANOS_PER_MS)
                                    moves.takeTouch(System.nanoTime(), gap)?.let { point ->
                                        sendKeyed(keyed, pan.moveAction, point, travel)
                                    }
                                }
                            } else if (GestureRules.movesNow(at, lastMoveAt, pan.moveIntervalMs)) {
                                lastMoveAt = at
                                send(pan.moveAction, JsGesturePhase.MOVED, x, y, travel)
                            }
                        }
                    }

                    var total = Offset.Zero
                    var owed = Offset.Zero
                    var catchUpFrame = 0
                    var claimed = false
                    var lifted = false

                    fun catchUpStep(): Offset {
                        val before = catchUpFrame
                        catchUpFrame += 1
                        return Offset(
                            GestureRules.caughtUp(owed.x, catchUpFrame) - GestureRules.caughtUp(owed.x, before),
                            GestureRules.caughtUp(owed.y, catchUpFrame) - GestureRules.caughtUp(owed.y, before),
                        )
                    }
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (change.changedToUpIgnoreConsumed()) {
                                lifted = true
                                if (claimed) {
                                    val rest = Offset(
                                        owed.x - GestureRules.caughtUp(owed.x, catchUpFrame),
                                        owed.y - GestureRules.caughtUp(owed.y, catchUpFrame),
                                    )
                                    if (rest != Offset.Zero) track(rest.x, rest.y, change.uptimeMillis)
                                    change.consume()
                                }
                                break
                            }
                            val delta = change.positionChangeIgnoreConsumed()
                            if (claimed) {
                                val catchUp = catchUpStep()
                                track(delta.x + catchUp.x, delta.y + catchUp.y, change.uptimeMillis)
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
                                    cutSettleShort()
                                    node.position?.let { position ->
                                        if (pan.moveAction != null) {
                                            moves.clear()
                                            moves.noteAnswer(position.value.value.answer)
                                            moveJob = scope.launchGestureMoves(moves, position, pan.moveIntervalMs) { point ->
                                                if (current.enabled && current.position === position) {
                                                    sendKeyed(position, pan.moveAction, point, travel)
                                                } else {
                                                    moves.clear()
                                                }
                                            }
                                        }
                                    }
                                    send(pan.startAction, JsGesturePhase.STARTED, x, y, travel)
                                    // From the touch-down point: the slop crossed before the claim is fed in over the
                                    // first frames, so the children catch up with the finger instead of jumping to it.
                                    owed = when (pan.axis) {
                                        JsPanAxis.HORIZONTAL -> Offset(sign(total.x) * slop, 0f)
                                        JsPanAxis.VERTICAL -> Offset(0f, sign(total.y) * slop)
                                        JsPanAxis.FREE -> Offset.Zero
                                    }
                                    val catchUp = catchUpStep()
                                    track(total.x - owed.x + catchUp.x, total.y - owed.y + catchUp.y, change.uptimeMillis)
                                    change.consume()
                                }
                                SlideDragDecision.RELEASE -> break
                                SlideDragDecision.WAIT -> Unit
                            }
                        }
                    } finally {
                        moveJob?.cancel()
                        moves.clear()
                        // A press the drag did not complete lets go; a completion's pop takes it over.
                        if (!claimed || drag == null) unpress()
                        when {
                            claimed && pan != null -> {
                                if (drag == null && node.position != null && pan.moveAction != null) {
                                    val along = if (pan.axis == JsPanAxis.VERTICAL) y else x
                                    settleWorkerThumb(pan.axis, GestureRules.permille(along, travel), cancelled = !lifted, travel)
                                }
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
                                        unpress()
                                    }
                                    GestureSettle.COMPLETE -> complete(travel)
                                    GestureSettle.SNAP_BACK -> {
                                        send(pan.endAction, JsGesturePhase.ENDED, x, y, travel)
                                        val feel = current.feel
                                        moveTo(0f, 0f, animated = true, spec = feel?.springHome()?.fromLift() ?: SpringBack, dips = feel != null)
                                        unpress()
                                    }
                                    GestureSettle.STAY -> {
                                        send(pan.endAction, JsGesturePhase.ENDED, x, y, travel)
                                        unpress()
                                    }
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
                    val position = GestureRules.newest(
                        workerPosition?.value,
                        answeredInFrame?.takeIf { it.first === current.position }?.second,
                    )
                    val worker = drawnWorkerPx(travel)
                    val workerX = worker.x
                    val workerY = worker.y
                    val dips = dippingMove == moveNumber
                    IntOffset(
                        (WorkerThumbSettle.along(placeX.value, travel, dips) + workerX).roundToInt(),
                        (WorkerThumbSettle.along(placeY.value, travel, dips) + workerY).roundToInt(),
                    ).also { placed ->
                    }
                }
                .graphicsLayer {
                    val pop = popped
                    val scale = pop?.first ?: pressScale.value
                    scaleX = scale
                    scaleY = scale
                    // About the drawn child's centre, not the margin the worker's tree put before it.
                    val base = basePx()
                    transformOrigin = TransformOrigin(
                        if (childrenWidth > 0) (base + (childrenWidth - base) / 2f) / childrenWidth else 0.5f,
                        0.5f,
                    )
                    translationX = (pop?.second ?: 0f) * density * if (rtl) -1f else 1f
                },
        ) {
            content()
        }
    }
}

private const val NANOS_PER_MS = 1_000_000L
private const val NANOS_PER_FRAME = 1_000_000_000f / 60f
private const val PRESS_REACH_DP = 10f

/** The feel's damped spring home: its natural frequency squared is the stiffness. */
private fun JsPanFeel.springHome(): AnimationSpec<Float> = spring(dampingRatio = springDamping, stiffness = springStiffness())

/**
 * How a released every-move thumb settles: with the pan's feel when it has one (its spring home, or its ease-out
 * approach over [distanceDp] to the end), else the native slide's spring and a short ease to the end.
 */
internal fun settleSpec(kind: WorkerThumbSettle.Kind, feel: JsPanFeel?, distanceDp: Float): AnimationSpec<Offset> = when {
    feel == null -> if (kind == WorkerThumbSettle.Kind.END) SettleToEnd else SpringBackOffset
    kind == WorkerThumbSettle.Kind.END ->
        tween(WidgetStyleRules.approachMs(feel, distanceDp), easing = WidgetStyleRules.EaseOutQuad)
    else -> spring(dampingRatio = feel.springDamping, stiffness = feel.springStiffness())
}

private fun JsPanFeel.springStiffness(): Float = (springOmega * springOmega).coerceAtLeast(Spring.StiffnessVeryLow)

private val SpringBack = spring<Float>(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMediumLow)
private val SpringBackOffset = spring<Offset>(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMediumLow)
private val SettleToEnd = tween<Offset>(durationMillis = WorkerThumbSettle.END_MILLIS, easing = LinearOutSlowInEasing)
