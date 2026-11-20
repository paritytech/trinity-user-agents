import SwiftUI

struct NodeGestureView<Content: View>: View {
    let props: CustomMessageWidgetNode.GestureProps
    let modifiers: CustomMessageWidgetNode.Modifiers
    @ViewBuilder let content: () -> Content

    @Environment(\.widgetActionHandler) private var actionHandler
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

    var body: some View {
        content()
            .onGeometryChange(for: CGFloat.self, of: \.size.width) { childWidth = $0 }
            .offset(x: offset.x * (layoutDirection == .rightToLeft ? -1 : 1), y: offset.y)
            .frame(maxWidth: .infinity, alignment: .leading)
            .onGeometryChange(for: CGFloat.self, of: \.size.width) { width = $0 }
            .overlay {
                GestureSurface(props: interactiveProps, onPan: receive, onTap: tap)
            }
            .accessibilityElement(children: props.accessibilityLabel == nil ? .contain : .ignore)
            .accessibilityLabel(props.accessibilityLabel ?? "")
            .accessibilityAddTraits(.isButton)
            .accessibilityAction { activate() }
            .sensoryFeedback(.success, trigger: confirmations)
            .applyWidgetNodeModifiers(modifiers, alignment: .leading)
            .onChange(of: props.isEnabled) { _, enabled in
                if !enabled, dragging { cancel() }
                if enabled { held = false; offset = .zero }
            }
    }
}

private extension NodeGestureView {
    var interactiveProps: CustomMessageWidgetNode.GestureProps {
        var value = props
        value.isEnabled = value.isEnabled && !held
        return value
    }

    var travel: CGFloat { max(props.pan?.drag?.travelPoints ?? (width - childWidth), 0) }

    func emit(_ action: String?, _ phase: CustomMessageWidgetNode.GesturePhase) {
        guard let action else { return }
        let fraction = GestureMotion.fraction(position, axis: props.pan?.axis ?? .horizontal, travel: panTravel)
        actionHandler?(action, props.encodePayload(phase, position, fraction))
    }

    func receive(_ phase: CustomMessageWidgetNode.GesturePhase, _ translation: PanTranslation) {
        guard props.isEnabled, !held, let pan = props.pan else { return }
        if case .started = phase {
            dragging = true
            panTravel = travel
            origin = pan.drag == nil ? .zero : offset
            position = origin
            lastMove = 0
            catchUpStep = 0
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
        if pan.drag != nil { offset = position }
        switch phase {
        case .started,
             .moved:
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

    func release(cancelled: Bool) {
        guard let pan = props.pan else { return }
        dragging = false
        let completed = !cancelled && GestureMotion.completes(
            position, axis: pan.axis, travel: panTravel, release: pan.drag?.release
        )
        emit(pan.endAction, cancelled ? .cancelled : completed ? .completed : .ended)
        guard let drag = pan.drag else { return }
        let animation: Animation? = reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.6)
        withAnimation(animation) {
            if completed {
                held = drag.holdAtEnd
                offset = held ? GestureMotion.end(axis: pan.axis, travel: panTravel) : .zero
            } else if cancelled {
                offset = origin
            } else {
                switch drag.release {
                case .stay: break
                case .snapBack,
                     .complete: offset = .zero
                }
            }
        }
        if completed, props.hapticOnComplete { confirmations += 1 }
    }

    func cancel() {
        release(cancelled: true)
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
    /// thumb catches up with the finger instead of jumping to it.
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
}
