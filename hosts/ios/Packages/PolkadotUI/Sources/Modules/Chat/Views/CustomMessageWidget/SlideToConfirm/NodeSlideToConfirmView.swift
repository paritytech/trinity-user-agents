import SwiftUI

/// Owns the slide: the drag, where it settles, the one confirmation it fires
/// and what VoiceOver is told. How the control looks at a given progress is the
/// face's, one per variant.
struct NodeSlideToConfirmView: View {
    let props: CustomMessageWidgetNode.SlideToConfirmProps
    let modifiers: CustomMessageWidgetNode.Modifiers

    @Environment(\.widgetActionHandler) private var actionHandler
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var track: SlideToConfirmTrack
    @State private var width: CGFloat = 0
    @State private var confirmations = 0
    /// The existing slider's press: the thumb grown while a finger holds it.
    @State private var pressed = false
    /// Its pop after a confirmed slide: when it starts and the scale it starts from.
    @State private var pop: (start: Date, from: CGFloat)?

    init(props: CustomMessageWidgetNode.SlideToConfirmProps, modifiers: CustomMessageWidgetNode.Modifiers) {
        self.props = props
        self.modifiers = modifiers
        _track = State(initialValue: SlideToConfirmTrack(heldAtEnd: props.isHeldAtEnd))
    }

    var body: some View {
        TimelineView(.animation(paused: pop == nil)) { timeline in
            let popped = popState(at: timeline.date)
            SlideToConfirmFace(
                props: props,
                progress: track.progress,
                // The shimmer band runs while the slider waits, is dragged or springs home; not once it is held.
                isHinting: props.acceptsSlide && !track.isHeldAtEnd,
                trackHeight: SlideToConfirmMetrics.thumbTrackHeight(given: modifiers),
                thumbScale: popped?.scale ?? (pressed ? feel.pressScale : 1),
                thumbPast: popped?.past ?? 0
            )
        }
        .overlay {
            SlideToConfirmDragSurface(isInteractive: isInteractive, onChange: follow, onEnd: release, onPress: press)
        }
        .onGeometryChange(for: CGFloat.self, of: \.size.width) { width = $0 }
        .opacity(props.isEnabled ? 1 : SlideToConfirmMetrics.disabledOpacity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { confirm() }
        .disabled(!isInteractive)
        .sensoryFeedback(.success, trigger: confirmations)
        .onChange(of: props.isHeldAtEnd) { _, holds in
            withAnimation(settleAnimation) { track.follow(productHoldsAtEnd: holds) }
        }
        .applyWidgetNodeModifiers(modifiers)
    }
}

private extension NodeSlideToConfirmView {
    var isInteractive: Bool {
        props.acceptsSlide && !track.isHeldAtEnd
    }

    var travel: CGFloat {
        SlideToConfirmMetrics.travel(
            of: props.variant,
            width: width,
            trackHeight: SlideToConfirmMetrics.thumbTrackHeight(given: modifiers)
        )
    }

    var accessibilityLabel: String {
        [props.displayedText, props.secondaryText].compactMap { $0 }.joined(separator: ", ")
    }

    var feel: CustomMessageWidgetNode.PanFeel { SlideStyle.feel }

    /// The existing slider's damped spring home (18 rad/s, 0.8), from rest.
    var springBack: Animation? {
        reduceMotion ? nil : feel.springHome
    }

    var settleAnimation: Animation? {
        reduceMotion ? nil : .easeOut(duration: 0.16)
    }

    func follow(_ translation: CGFloat) {
        guard isInteractive else { return }
        track.drag(translation: translation, travel: travel)
    }

    func release(lifted: Bool) {
        var settled = track
        let from = track.progress
        let outcome = settled.release(lifted: lifted)
        if outcome == .confirm {
            finish(to: settled, from: from)
            fireConfirmAction()
            return
        }
        withAnimation(springBack) { track = settled }
        unpress()
    }

    func confirm() {
        guard isInteractive else { return }
        var settled = track
        let from = track.progress
        guard settled.confirm() == .confirm else { return }
        finish(to: settled, from: from)
        fireConfirmAction()
    }

    /// The existing slider's finish: the thumb eases out to the end (117 ms, longer over a longer way) and pops as it arrives.
    func finish(to settled: SlideToConfirmTrack, from progress: CGFloat) {
        guard !reduceMotion else {
            pressed = false
            track = settled
            return
        }
        let approach = feel.approach(distance: (1 - progress) * travel)
        withAnimation(CustomMessageWidgetNode.PanFeel.easeOutQuad(approach)) { track = settled }
        pop = (Date().addingTimeInterval(approach - feel.approachDuration), pressed ? feel.pressScale : 1)
        pressed = false
    }

    /// A finger down on the thumb (within 10 points of it, as the existing slider takes it) grows it; lifting without a slide
    /// lets it go.
    func press(_ down: Bool, at location: CGPoint) {
        guard props.variant == .thumb, isInteractive, pop == nil else { return }
        guard down else {
            if !track.isDragging { unpress() }
            return
        }
        let trackHeight = SlideToConfirmMetrics.thumbTrackHeight(given: modifiers)
        let inset = SlideToConfirmMetrics.thumbInset(trackHeight: trackHeight)
        let radius = SlideToConfirmMetrics.thumbSize / 2
        let center = CGPoint(x: inset + radius + track.progress * travel, y: trackHeight / 2)
        let dx = location.x - center.x, dy = location.y - center.y
        guard dx * dx + dy * dy <= (radius + 10) * (radius + 10) else { return }
        withAnimation(reduceMotion ? nil : feel.press) { pressed = true }
    }

    func unpress() {
        guard pressed else { return }
        withAnimation(reduceMotion ? nil : feel.unpress) { pressed = false }
    }

    func popState(at date: Date) -> (scale: CGFloat, past: CGFloat)? {
        guard let pop else { return nil }
        if let state = feel.pop(at: date.timeIntervalSince(pop.start) * 60, from: pop.from) { return state }
        Task { @MainActor in self.pop = nil }
        return nil
    }

    func fireConfirmAction() {
        confirmations += 1
        if let actionId = props.confirmAction {
            actionHandler?(actionId, nil)
        }
    }
}

enum SlideToConfirmMetrics {
    static let trackHeight: CGFloat = 72
    static let thumbSize: CGFloat = 56
    static let iconSize: CGFloat = 24
    /// What a full reveal slide leaves uncovered at the end: the last of the peeled face.
    static let travelEndInset: CGFloat = 56
    static let disabledOpacity: CGFloat = 0.4
    /// The label is gone once the thumb has travelled this share of the track.
    static let labelGoneAt: CGFloat = 0.6
    static let revealHeight: CGFloat = 96

    /// How tall the thumb variant's track stands: the height the product gave the node, such as the
    /// room footer's 80 that every docked card shares, or the track's own. Never less than the thumb,
    /// which the track has to hold.
    static func thumbTrackHeight(given modifiers: CustomMessageWidgetNode.Modifiers) -> CGFloat {
        max(modifiers.height ?? trackHeight, thumbSize)
    }

    /// The space around the thumb inside a track, the same on every side: 8 in the track's own 72,
    /// 12 in the room footer's 80, as on the web footer's pill.
    static func thumbInset(trackHeight: CGFloat) -> CGFloat {
        (trackHeight - thumbSize) / 2
    }

    /// How far a full slide goes on a control `width` wide. The drag divides the finger's move by
    /// this and the face draws progress over the same distance, so the thumb follows the finger one
    /// for one. The thumb travels the lane inside the track's inset; the reveal keeps its own end.
    static func travel(
        of variant: CustomMessageWidgetNode.SlideToConfirmVariant,
        width: CGFloat,
        trackHeight: CGFloat
    ) -> CGFloat {
        switch variant {
        case .thumb:
            thumbTravel(laneWidth: width - 2 * thumbInset(trackHeight: trackHeight))
        case .reveal:
            max(width - travelEndInset, 0)
        }
    }

    /// How far the thumb travels along a lane `laneWidth` wide: the track inside its inset.
    static func thumbTravel(laneWidth: CGFloat) -> CGFloat {
        max(laneWidth - thumbSize, 0)
    }
}
