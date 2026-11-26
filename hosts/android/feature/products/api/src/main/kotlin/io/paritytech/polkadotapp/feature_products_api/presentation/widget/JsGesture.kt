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
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChangeIgnoreConsumed
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.layout.positionInRoot
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
 * The recogniser sits on the node, which never moves, and the children are offset inside it. The travel is taken when a pan starts, so a product that moves the children
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
    var layoutCoordinates by remember { mutableStateOf<LayoutCoordinates?>(null) }

    fun placeTrail(coordinates: LayoutCoordinates) {
        current.trail?.let { trail ->
            trail.origin = coordinates.positionInRoot()
            trail.coordinates = coordinates
        }
    }

    // Replacing a trail at unchanged bounds does not trigger another layout callback.
    SideEffect { layoutCoordinates?.takeIf { it.isAttached }?.let(::placeTrail) }

    // Host-owned place of the children, in px from the travel's start. Kept across trees at the same place.
    val placeX = remember { Animatable(0f) }
    val placeY = remember { Animatable(0f) }
    var nodeWidth by remember { mutableIntStateOf(0) }
    var nodeHeight by remember { mutableIntStateOf(0) }
    var childrenWidth by remember { mutableIntStateOf(0) }
    var heldAtEnd by remember { mutableStateOf(false) }
    // An every-move pan's settle after the finger lifts (PanReleaseSettle), drawn instead of the worker's place.
    val settle = remember { Animatable(Offset.Zero, Offset.VectorConverter) }
    var settling by remember { mutableStateOf(false) }
    var settleJob by remember { mutableStateOf<Job?>(null) }
    // A release home on a feel's spring may dip below home, as the spring does (PanReleaseSettle.onLane):
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
    val pressScale = remember { Animatable(Offset(1f, 1f), Offset.VectorConverter) }
    var pressJob by remember { mutableStateOf<Job?>(null) }
    var popped by remember { mutableStateOf<Pair<Float, Float>?>(null) }
    var poppedScaleY by remember { mutableStateOf<Float?>(null) }
    var popJob by remember { mutableStateOf<Job?>(null) }

    /** Where the worker's tree drew the children: the first child's leading margin, in px. */
    fun basePx(): Float = (current.children.firstOrNull()?.modifiers().orEmpty().filterIsInstance<JsModifier.Margin>().firstOrNull()
        ?.let { it.start ?: it.horizontal ?: it.all } ?: 0) * density

    /** Where the worker's place draws the children now, in px, as the offset below lays them out. */
    fun drawnWorkerPx(travel: Float): Offset {
        if (settling) return PanReleaseSettle.onLane(settle.value, travel, settleDips)
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

    /** Where the children are drawn from where the tree put them, in px along the layout direction and down. */
    fun drawnOffsetPx(): Offset {
        val travel = travelNow()
        val worker = drawnWorkerPx(travel)
        val dips = dippingMove == moveNumber
        return Offset(PanReleaseSettle.along(placeX.value, travel, dips) + worker.x, PanReleaseSettle.along(placeY.value, travel, dips) + worker.y)
    }

    widget.key?.let { key ->
        val place = LocalGesturePanPlaces.current.place(key)
        SideEffect {
            place.progress = ::panProgress
            // Including a completed drag's pop past the end, as the children are drawn.
            place.offset = { drawnOffsetPx() + Offset((popped?.second ?: 0f) * density, 0f) }
        }
    }

    fun press(feel: JsPanFeel) {
        pressJob?.cancel()
        pressJob = scope.launch { pressScale.animateTo(GestureRules.pressScale(feel), tween(feel.pressMs, easing = WidgetStyleRules.EaseOutQuad)) }
    }

    fun unpress() {
        val feel = current.feel ?: return
        if (pressScale.targetValue == Offset(1f, 1f) && !pressScale.isRunning && pressScale.value == Offset(1f, 1f)) return
        pressJob?.cancel()
        pressJob = scope.launch { pressScale.animateTo(Offset(1f, 1f), tween(feel.unpressMs, easing = WidgetStyleRules.EaseOutQuad)) }
    }

    /** A finger down on the children where they are drawn, or within [PanPress.REACH_DP] of them. */
    fun hitsChild(at: Offset): Boolean {
        val base = basePx()
        val width = childrenWidth - base
        if (width <= 0f) return false
        val worker = drawnWorkerPx(travelNow())
        val fromStart = base + placeX.value.coerceIn(0f, travelNow()) + worker.x
        val top = placeY.value.coerceIn(0f, travelNow()) + worker.y
        val start = if (rtl) nodeWidth - fromStart - width else fromStart
        return PanPress.reaches(at, start, top, width, nodeHeight.toFloat(), PanPress.REACH_DP * density)
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
                    popped = WidgetStyleRules.pop(feel, (now - start) / NANOS_PER_FRAME, from.x) ?: break
                    poppedScaleY = WidgetStyleRules.pop(feel, (now - start) / NANOS_PER_FRAME, from.y)?.first
                }
            } finally {
                popped = null
                poppedScaleY = null
                scope.launch { pressScale.snapTo(Offset(1f, 1f)) }
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
            // The answer was written on the render stream's thread; registering it now, on the main thread, makes
            // the draw that reads it (a mask's added shapes) part of this frame.
            Snapshot.sendApplyNotifications()
        }
    }

    /** Where the worker's place draws the children now, in px from the travel's start. */
    fun workerPlacePx(): Offset {
        if (settling) return settle.value
        val shown = GestureRules.newest(
            current.position?.value?.value,
            answeredInFrame?.takeIf { it.first === current.position }?.second,
        ) ?: return Offset.Zero
        return Offset(shown.x / 100f * density, shown.y / 100f * density)
    }

    /** Released every-move children settle from where they are drawn instead of jumping to the worker's next place. */
    fun settleWorkerChildren(axis: JsPanAxis, releasedPermille: Int, cancelled: Boolean, travel: Float) {
        if (!PanReleaseSettle.currentIsOn) return
        val from = workerPlacePx()
        val kind = PanReleaseSettle.kind(axis, releasedPermille, cancelled)
        val to = PanReleaseSettle.target(kind, axis, travel)
        if (!PanReleaseSettle.animates(ValueAnimator.areAnimatorsEnabled(), from, to)) return
        settleJob?.cancel()
        settling = true
        settleDips = kind == PanReleaseSettle.Kind.HOME && current.feel != null
        settleJob = scope.launch(start = CoroutineStart.UNDISPATCHED) {
            try {
                settle.snapTo(from)
                val spec = settleSpec(kind, current.feel, (to - from).getDistance() / density)
                // With the feel's motion, timed from the lift (FromLift).
                settle.animateTo(to, if (current.feel != null) spec.fromLift() else spec)
            } finally {
                settling = false
            }
        }
    }

    /** A new drag takes the children over at once. */
    fun cutSettleShort() {
        settleJob?.cancel()
        settleJob = null
        settling = false
    }

    /** [dips]: a release home on a feel's spring, drawn dipping below home as the spring does. */
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
            // The feel's finish: an ease-out to the end, longer over a longer way, and the pop as the children arrive.
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
            // Where the pan's strokes sit on screen, for masks that follow them.
            .onGloballyPositioned { coordinates ->
                layoutCoordinates = coordinates
                placeTrail(coordinates)
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
            // The loop reads every move itself, consumed or not, and consumes what it uses, so a drag that
            // starts on the node is the node's alone and swipe to reply and the feed's scroll stand down.
            .pointerInput(Unit) {
                // Lower than the system slop by default, so the moved child follows after about 1 mm (GestureDragThreshold).
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
                    if (pan != null && pan.location) {
                        // Where the finger is, not how far it moved: Started carries the touch-down point, every move
                        // the finger's place (paced like any keyed pan), and the end the place it lifted. Nothing is
                        // clamped to travel and nothing completes.
                        val width = nodeWidth.toFloat()
                        fun at(point: Offset): Offset =
                            GestureRules.inside(point.x, point.y, nodeWidth, nodeHeight, rtl).let { Offset(it.first, it.second) }

                        // The trail keeps places from the pan's left edge, whatever the layout direction. A place
                        // joins it at its touch, before the move is offered, so a mask following the pan draws it in
                        // this frame.
                        fun leftToRight(point: Offset): Offset = if (rtl) Offset(width - point.x, point.y) else point
                        fun trace(point: Offset, begins: Boolean = false) {
                            val trail = node.trail ?: return
                            if (begins) trail.begin(leftToRight(point)) else trail.extend(leftToRight(point))
                            Snapshot.sendApplyNotifications()
                        }
                        var total = Offset.Zero
                        var claimed = false
                        var lifted = false
                        var last = at(down.position)
                        var lastMoveAt: Long? = null
                        var moveJob: Job? = null
                        fun offer(point: Offset, at: Long) {
                            pan.moveAction ?: return
                            val keyed = node.position
                            if (keyed != null) {
                                moves.offer(point)
                                // Like any keyed pan: the move goes out at its touch, and its answer (a mask's shapes,
                                // say) is waited for briefly so this frame draws it.
                                if (GesturePacingSettings.currentSendOnTouch && current.enabled && current.position === keyed) {
                                    moves.noteAnswer(keyed.value.value.answer)
                                    val gap = maxOf(minimumGapNanos, pan.moveIntervalMs * NANOS_PER_MS)
                                    moves.takeTouch(System.nanoTime(), gap)?.let { move ->
                                        sendKeyed(keyed, pan.moveAction, move, width)
                                    }
                                }
                            } else if (GestureRules.movesNow(at, lastMoveAt, pan.moveIntervalMs)) {
                                lastMoveAt = at
                                send(pan.moveAction, JsGesturePhase.MOVED, point.x, point.y, width)
                            }
                        }

                        // The pan takes the finger: Started at the touch-down point, then this change's place.
                        fun claim(change: PointerInputChange) {
                            claimed = true
                            node.position?.let { position ->
                                if (pan.moveAction != null) {
                                    moves.clear()
                                    moves.noteAnswer(position.value.value.answer)
                                    moveJob = scope.launchGestureMoves(moves, position, pan.moveIntervalMs) { point ->
                                        if (current.enabled && current.position === position) {
                                            sendKeyed(position, pan.moveAction, point, width)
                                        } else {
                                            moves.clear()
                                        }
                                    }
                                }
                            }
                            val start = at(down.position)
                            last = at(change.position)
                            node.trail?.touch(isDown = true)
                            trace(start, begins = true)
                            trace(last)
                            send(pan.startAction, JsGesturePhase.STARTED, start.x, start.y, width)
                            offer(last, change.uptimeMillis)
                            change.consume()
                        }
                        // A free location pan starts where the finger lands, so a press with no movement still leaves
                        // its mark: a following mask draws a dot.
                        if (GestureRules.claimsAtTouchDown(pan.axis, pan.location)) claim(down)
                        try {
                            while (true) {
                                val event = awaitPointerEvent()
                                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                                if (change.changedToUpIgnoreConsumed()) {
                                    lifted = !event.isCancel()
                                    if (claimed && lifted) {
                                        last = at(change.position)
                                        trace(last)
                                        change.consume()
                                    }
                                    break
                                }
                                if (claimed) {
                                    last = at(change.position)
                                    trace(last)
                                    offer(last, change.uptimeMillis)
                                    change.consume()
                                    continue
                                }
                                total += change.positionChangeIgnoreConsumed()
                                when (GestureRules.decide(pan.axis, total.x, total.y, slop)) {
                                    PanDecision.CLAIM -> claim(change)
                                    PanDecision.RELEASE -> break
                                    PanDecision.WAIT -> Unit
                                }
                            }
                        } finally {
                            moveJob?.cancel()
                            moves.clear()
                            unpress()
                            // After the last place joined the trail, so a mask checking its share on the lift counts it.
                            node.trail?.touch(isDown = false)
                            if (claimed) {
                                send(pan.endAction, if (lifted) JsGesturePhase.ENDED else JsGesturePhase.CANCELLED, last.x, last.y, width)
                            } else if (lifted && total.getDistance() <= slop) {
                                send(node.tapAction, JsGesturePhase.TAP, 0f, 0f, width)
                            }
                        }
                        return@awaitEachGesture
                    }
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
                                lifted = !event.isCancel()
                                if (claimed && lifted) {
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
                                PanDecision.CLAIM -> {
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
                                PanDecision.RELEASE -> break
                                PanDecision.WAIT -> Unit
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
                                    settleWorkerChildren(pan.axis, GestureRules.permille(along, travel), cancelled = !lifted, travel)
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
                    val drawn = drawnOffsetPx()
                    IntOffset(drawn.x.roundToInt(), drawn.y.roundToInt())
                }
                .graphicsLayer {
                    val pop = popped
                    scaleX = pop?.first ?: pressScale.value.x
                    scaleY = poppedScaleY ?: pressScale.value.y
                    current.trail?.drawnScale = Offset(scaleX, scaleY)
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

/**
 * Whether this event is a touch the system took away (palm rejection, a system gesture, a parent taking the touch)
 * rather than a lift. Compose reports such a cancel as an up it builds itself, with no MotionEvent behind it; a real
 * lift always carries one, even when a child pan inside already consumed it.
 */
private fun PointerEvent.isCancel(): Boolean = motionEvent == null

/** The feel's damped spring home: its natural frequency squared is the stiffness. */
private fun JsPanFeel.springHome(): AnimationSpec<Float> = spring(dampingRatio = springDamping, stiffness = springStiffness())

/**
 * How released every-move children settle: with the pan's feel when it has one (its spring home, or its ease-out
 * approach over [distanceDp] to the end), else the host's return spring and a short ease to the end.
 */
internal fun settleSpec(kind: PanReleaseSettle.Kind, feel: JsPanFeel?, distanceDp: Float): AnimationSpec<Offset> = when {
    feel == null -> if (kind == PanReleaseSettle.Kind.END) SettleToEnd else SpringBackOffset
    kind == PanReleaseSettle.Kind.END ->
        tween(WidgetStyleRules.approachMs(feel, distanceDp), easing = WidgetStyleRules.EaseOutQuad)
    else -> spring(dampingRatio = feel.springDamping, stiffness = feel.springStiffness())
}

private fun JsPanFeel.springStiffness(): Float = (springOmega * springOmega).coerceAtLeast(Spring.StiffnessVeryLow)

private val SpringBack = spring<Float>(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMediumLow)
private val SpringBackOffset = spring<Offset>(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMediumLow)
private val SettleToEnd = tween<Offset>(durationMillis = PanReleaseSettle.END_MILLIS, easing = LinearOutSlowInEasing)
