import SwiftUI

struct NodeGestureView<Content: View>: View {
    let props: CustomMessageWidgetNode.GestureProps
    let modifiers: CustomMessageWidgetNode.Modifiers
    /// Where the worker's tree drew the children: the first child's leading margin.
    var childBase: CGFloat = 0
    @ViewBuilder let content: () -> Content

    @Environment(\.widgetActionHandler) private var actionHandler
    @Environment(\.gestureOffsets) private var gestureOffsets
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.layoutDirection) private var layoutDirection
    @State private var width: CGFloat = 0
    @State private var childWidth: CGFloat = 0
    @State private var offset = CGPoint.zero
    @State private var origin = CGPoint.zero
    @State private var position = CGPoint.zero
    @State private var panTravel: CGFloat = 0
    @State private var dragging = false
    @State private var held = false
    @State private var lastMove: TimeInterval = 0
    @State private var catchUpStep = 0
    @State private var confirmations = 0
    @State private var pacer = GestureMovePacer()
    /// Bumped when a pan starts while a release settle still moves the children, dropping that animation at once.
    @State private var settleGeneration = 0
    @State private var height: CGFloat = 0
    /// The feel's press: the children grown while a finger holds them.
    @State private var pressed = false
    /// A completed host drag's pop: when it starts and the scale it starts from.
    @State private var pop: (start: Date, from: CGFloat)?

    var body: some View {
        TimelineView(.animation(paused: pop == nil)) { timeline in
            let popped = popState(at: timeline.date)
            content()
                .onGeometryChange(for: CGFloat.self, of: \.size.width) { childWidth = $0 }
                .scaleEffect(popped?.scale ?? (pressed ? props.feel?.pressScale ?? 1 : 1), anchor: scaleAnchor)
                .offset(
                    x: (shown.x + (popped?.past ?? 0)) * (layoutDirection == .rightToLeft ? -1 : 1),
                    y: shown.y
                )
        }
        .id(settleGeneration)
        .frame(maxWidth: .infinity, alignment: .leading)
        .onGeometryChange(for: CGSize.self, of: \.size) { width = $0.width; height = $0.height }
        .onGeometryChange(for: CGPoint.self) { $0.frame(in: .global).origin } action: { trail?.origin = $0 }
        .onChange(of: width) { updatePlace() }
        .onChange(of: childWidth) { updatePlace() }
        .onChange(of: childBase) { updatePlace() }
        .onAppear { updatePlace() }
        .overlay {
            GestureSurface(props: interactiveProps, onPan: receive, onTap: tap, onPress: press)
        }
        .accessibilityElement(children: props.accessibilityLabel == nil ? .contain : .ignore)
        .accessibilityLabel(props.accessibilityLabel ?? "")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { activate() }
        .sensoryFeedback(.success, trigger: confirmations)
        .applyWidgetNodeModifiers(modifiers, alignment: .leading)
        .onChange(of: props.isEnabled) { _, enabled in
            if !enabled, dragging { cancel() }
            if enabled { held = false; setOffset(.zero) }
        }
        .onDisappear { pacer.stop() }
    }
}

private extension NodeGestureView {
    var interactiveProps: CustomMessageWidgetNode.GestureProps {
        var value = props
        value.isEnabled = value.isEnabled && !held
        return value
    }

    var travel: CGFloat { max(props.pan?.drag?.travelPoints ?? (width - childWidth), 0) }

    /// The worker's offset for this keyed gesture, read here alone so an update draws only this view.
    var channel: GestureOffsetChannel? { props.key.flatMap { gestureOffsets?.channel(for: $0) } }

    /// Where the children are drawn: the host's own drag offset plus any offset the worker set since its last tree.
    var shown: CGPoint {
        let set = channel?.offset ?? .zero
        return CGPoint(x: offset.x + set.x, y: offset.y + set.y)
    }

    /// The finger's strokes on this keyed location pan, for masks that follow it.
    var trail: PanTrail? {
        guard props.pan?.reportsLocation == true else { return nil }
        return props.key.flatMap { gestureOffsets?.trail(for: $0) }
    }

    /// A place the surface reports (from its top-start corner) from the pan's top-left corner, as the trail keeps it.
    func leftToRight(_ point: CGPoint) -> CGPoint {
        layoutDirection == .rightToLeft ? CGPoint(x: width - point.x, y: point.y) : point
    }

    /// A keyed pan whose worker hears its moves: those are paced instead of sent one by one.
    var pacesMoves: Bool { props.key != nil && props.pan?.moveAction != nil }

    func payload(_ phase: CustomMessageWidgetNode.GesturePhase) -> Data {
        let fraction = props.pan?.reportsLocation == true
            ? GestureMotion.fraction(position, axis: .horizontal, travel: width)
            : GestureMotion.fraction(position, axis: props.pan?.axis ?? .horizontal, travel: panTravel)
        return props.encodePayload(phase, position, fraction)
    }

    func emit(_ action: String?, _ phase: CustomMessageWidgetNode.GesturePhase) {
        guard let action else { return }
        actionHandler?(action, payload(phase))
    }

    func startPacing(_ pan: CustomMessageWidgetNode.PanGesture) {
        guard pacesMoves, let moveAction = pan.moveAction else { return }
        let handler = actionHandler
        pacer.start(send: { handler?(moveAction, $0) }, offsets: gestureOffsets)
        channel?.onAnswer = { [pacer] in pacer.answered() }
    }

    func stopPacing() {
        pacer.stop()
        channel?.onAnswer = nil
    }

    func receive(_ phase: CustomMessageWidgetNode.GesturePhase, _ translation: PanTranslation) {
        guard props.isEnabled, !held, let pan = props.pan else { return }
        if pan.reportsLocation {
            receiveLocation(phase, translation, pan)
            return
        }
        if case .started = phase {
            cutSettleShort()
            dragging = true
            panTravel = travel
            origin = pan.drag == nil ? .zero : offset
            position = origin
            lastMove = 0
            catchUpStep = 0
            startPacing(pan)
            emit(pan.startAction, .started)
        }
        guard dragging else { return }
        switch phase {
        case .started,
             .moved: catchUpStep += 1
        default: catchUpStep = GestureMotion.catchUpSteps
        }
        position = GestureMotion.position(
            origin: origin,
            translation: GestureMotion.fromTouchDown(translation, step: catchUpStep),
            axis: pan.axis,
            travel: panTravel
        )
        if pan.drag != nil { setOffset(position) }
        switch phase {
        case .started,
             .moved:
            if pacesMoves {
                pacer.offer(payload(.moved), place: position)
                return
            }
            let now = ProcessInfo.processInfo.systemUptime
            if now - lastMove >= pan.moveInterval {
                lastMove = now
                emit(pan.moveAction, .moved)
            }
        case .ended: release(cancelled: false)
        case .cancelled: release(cancelled: true)
        default: break
        }
    }

    /// A pan that reports where the finger is: `Started` carries the touch-down point, every move the finger's place
    /// (paced like any keyed pan), and the end the place it lifted. Nothing is clamped to travel and nothing completes.
    /// Each place joins the pan's trail before the move is offered, so a mask following the pan draws it in this frame.
    func receiveLocation(
        _ phase: CustomMessageWidgetNode.GesturePhase,
        _ sample: PanTranslation,
        _ pan: CustomMessageWidgetNode.PanGesture
    ) {
        switch phase {
        case .started:
            dragging = true
            lastMove = 0
            startPacing(pan)
            trail?.begin(at: leftToRight(sample.touchDown))
            trail?.extend(to: leftToRight(sample.location))
            position = sample.touchDown
            emit(pan.startAction, .started)
            position = sample.location
            offerMove(pan)
        case .moved:
            guard dragging else { return }
            trail?.extend(to: leftToRight(sample.location))
            position = sample.location
            offerMove(pan)
        case .ended,
             .cancelled:
            guard dragging else { return }
            trail?.extend(to: leftToRight(sample.location))
            position = sample.location
            dragging = false
            stopPacing()
            emit(pan.endAction, phase)
        default: break
        }
    }

    func offerMove(_ pan: CustomMessageWidgetNode.PanGesture) {
        if pacesMoves {
            pacer.offer(payload(.moved))
            return
        }
        let now = ProcessInfo.processInfo.systemUptime
        if now - lastMove >= pan.moveInterval {
            lastMove = now
            emit(pan.moveAction, .moved)
        }
    }

    func release(cancelled: Bool) {
        guard let pan = props.pan else { return }
        dragging = false
        settleKeyedChildren(pan, cancelled: cancelled)
        stopPacing()
        let completed = !cancelled && GestureMotion.completes(
            position, axis: pan.axis, travel: panTravel, release: pan.drag?.release
        )
        emit(pan.endAction, cancelled ? .cancelled : completed ? .completed : .ended)
        guard let drag = pan.drag else {
            unpress()
            return
        }
        if completed, props.hapticOnComplete { confirmations += 1 }
        if let feel = props.feel, !reduceMotion {
            releaseWithFeel(feel, drag: drag, axis: pan.axis, completed: completed, cancelled: cancelled)
            return
        }
        let animation: Animation? = reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.6)
        pressed = false
        withAnimation(animation) {
            if completed {
                held = drag.holdAtEnd
                setOffset(held ? GestureMotion.end(axis: pan.axis, travel: panTravel) : .zero)
            } else if cancelled {
                setOffset(origin)
            } else {
                switch drag.release {
                case .stay: break
                case .snapBack,
                     .complete: setOffset(.zero)
                }
            }
        }
    }

    /// Every-move mode: the worker's children settle on the host when the finger lifts (`GestureSettle`), instead of
    /// jumping when the worker's end tree arrives.
    func settleKeyedChildren(_ pan: CustomMessageWidgetNode.PanGesture, cancelled: Bool) {
        guard pacesMoves, pan.drag == nil, let channel, let answer = channel.offset, let answered = pacer.sentPlace
        else { return }
        let along = pan.axis == .vertical ? position.y : position.x
        let kind = GestureSettle.kind(released: along, travel: panTravel, cancelled: cancelled)
        let target = GestureSettle.target(
            kind,
            axis: pan.axis,
            travel: panTravel,
            drawnAt: GestureSettle.drawnAt(answeredMove: answered, answer: answer)
        )
        channel.settle(
            to: target,
            animation: GestureSettle.animation(
                kind,
                reduceMotion: reduceMotion,
                feel: props.feel,
                remaining: max(panTravel - along, 0)
            ),
            now: ProcessInfo.processInfo.systemUptime
        )
    }

    /// A pan that starts while the children are still settling takes over at once.
    func cutSettleShort() {
        guard let channel else { return }
        if channel.isSettling(at: ProcessInfo.processInfo.systemUptime) { settleGeneration += 1 }
        channel.endSettle()
    }

    func cancel() {
        release(cancelled: true)
    }

    /// The host's drag offset, and the place the fading nodes read, in one transaction.
    func setOffset(_ value: CGPoint) {
        offset = value
        if let key = props
            .key { gestureOffsets?.place(for: key).drag = props.pan?.axis == .vertical ? value.y : value.x }
    }

    func updatePlace() {
        guard let key = props.key, let gestureOffsets else { return }
        let place = gestureOffsets.place(for: key)
        place.base = childBase
        place.travel = max(props.pan?.drag?.travelPoints ?? (width - childWidth + childBase), 0)
    }

    /// Scale about the drawn child's centre, not the margin the worker's tree put before it.
    var scaleAnchor: UnitPoint {
        guard childWidth > 0 else { return .center }
        return UnitPoint(x: (childBase + (childWidth - childBase) / 2) / childWidth, y: 0.5)
    }

    /// A finger down on the drawn children, or within `GestureMotion.pressReach` of them, grows them; lifting it
    /// without a pan lets them go.
    func press(_ down: Bool, at location: CGPoint) {
        guard let feel = props.feel, props.isEnabled, !held, pop == nil else { return }
        if !down {
            if !dragging { unpress() }
            return
        }
        let drawn = CGRect(x: childBase + shown.x, y: shown.y, width: childWidth - childBase, height: height)
        guard GestureMotion.reaches(location, drawn: drawn) else { return }
        withAnimation(reduceMotion ? nil : feel.press) { pressed = true }
    }

    func unpress() {
        guard pressed else { return }
        withAnimation(reduceMotion ? nil : props.feel?.unpress) { pressed = false }
    }

    /// A release with the feel: home on its damped spring, or to the end with its ease-out and pop.
    func releaseWithFeel(
        _ feel: CustomMessageWidgetNode.PanFeel,
        drag: CustomMessageWidgetNode.PanDrag,
        axis: CustomMessageWidgetNode.PanAxis,
        completed: Bool,
        cancelled: Bool
    ) {
        if completed {
            held = drag.holdAtEnd
            let end = held ? GestureMotion.end(axis: axis, travel: panTravel) : .zero
            let along = axis == .vertical ? position.y : position.x
            let approach = feel.approach(distance: panTravel - along)
            withAnimation(CustomMessageWidgetNode.PanFeel.easeOutQuad(approach)) { setOffset(end) }
            // The pop peaks as the children arrive; until it starts they keep the scale they were pressed to.
            let from = pressed ? feel.pressScale : 1
            pressed = false
            pop = (Date().addingTimeInterval(approach - feel.approachDuration), from)
            return
        }
        withAnimation(feel.springHome) {
            if cancelled {
                setOffset(origin)
            } else if case .stay = drag.release {
            } else {
                setOffset(.zero)
            }
        }
        unpress()
    }

    /// The pop's scale and the distance past the end at `date`, or nil when none plays.
    func popState(at date: Date) -> (scale: CGFloat, past: CGFloat)? {
        guard let pop, let feel = props.feel else { return nil }
        let frames = date.timeIntervalSince(pop.start) * 60
        if let state = feel.pop(at: frames, from: pop.from) { return state }
        Task { @MainActor in self.pop = nil }
        return nil
    }

    func tap() {
        guard props.isEnabled, !held else { return }
        position = .zero
        emit(props.tapAction, .tap)
    }

    func activate() {
        guard props.isEnabled, !held else { return }
        guard let pan = props.pan, case .complete = pan.drag?.release else { tap(); return }
        panTravel = travel
        guard panTravel > 0 else { return }
        position = GestureMotion.end(axis: pan.axis, travel: panTravel)
        origin = offset
        release(cancelled: false)
    }
}

enum GestureMotion {
    static let catchUpSteps = 3

    /// The translation from the touch-down point. The slop is fed in over the first `catchUpSteps` pan frames, so the
    /// moved child catches up with the finger instead of jumping to it.
    static func fromTouchDown(_ translation: PanTranslation, step: Int) -> CGPoint {
        let share = CGFloat(min(max(step, 0), catchUpSteps)) / CGFloat(catchUpSteps)
        return CGPoint(
            x: translation.recognized.x + translation.slop.x * share,
            y: translation.recognized.y + translation.slop.y * share
        )
    }

    static func position(
        origin: CGPoint, translation: CGPoint, axis: CustomMessageWidgetNode.PanAxis, travel: CGFloat
    ) -> CGPoint {
        let horizontal = min(max(origin.x + translation.x, 0), travel)
        let vertical = min(max(origin.y + translation.y, 0), travel)
        switch axis {
        case .horizontal: return CGPoint(x: horizontal, y: 0)
        case .vertical: return CGPoint(x: 0, y: vertical)
        case .free: return CGPoint(x: horizontal, y: vertical)
        }
    }

    static func fraction(_ point: CGPoint, axis: CustomMessageWidgetNode.PanAxis, travel: CGFloat) -> UInt16 {
        guard travel > 0 else { return 0 }
        let distance = axis == .vertical ? point.y : point.x
        return UInt16(min(max((distance / travel * 1_000).rounded(), 0), 1_000))
    }

    static func completes(
        _ point: CGPoint, axis: CustomMessageWidgetNode.PanAxis, travel: CGFloat,
        release: CustomMessageWidgetNode.PanRelease?
    ) -> Bool {
        guard travel > 0, case let .complete(threshold) = release else { return false }
        let distance = axis == .vertical ? point.y : point.x
        return distance >= travel * CGFloat(min(max(threshold, 1), 100)) / 100
    }

    static func end(axis: CustomMessageWidgetNode.PanAxis, travel: CGFloat) -> CGPoint {
        axis == .vertical ? CGPoint(x: 0, y: travel) : CGPoint(x: travel, y: 0)
    }

    /// How far around the drawn children a finger still presses them.
    static let pressReach: CGFloat = 10

    /// Whether a finger at `location` presses children drawn in `drawn`.
    static func reaches(_ location: CGPoint, drawn: CGRect) -> Bool {
        drawn.insetBy(dx: -pressReach, dy: -pressReach).contains(location)
    }
}
