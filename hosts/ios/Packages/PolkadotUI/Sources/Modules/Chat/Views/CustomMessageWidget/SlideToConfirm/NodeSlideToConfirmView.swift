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

    init(props: CustomMessageWidgetNode.SlideToConfirmProps, modifiers: CustomMessageWidgetNode.Modifiers) {
        self.props = props
        self.modifiers = modifiers
        _track = State(initialValue: SlideToConfirmTrack(heldAtEnd: props.isHeldAtEnd))
    }

    var body: some View {
        SlideToConfirmFace(
            props: props,
            progress: track.progress,
            isHinting: isInteractive && !track.isDragging,
            trackHeight: SlideToConfirmMetrics.thumbTrackHeight(given: modifiers)
        )
        .overlay {
            SlideToConfirmDragSurface(isInteractive: isInteractive, onChange: follow, onEnd: release)
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

    var springBack: Animation? {
        reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 0.6)
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
        let outcome = settled.release(lifted: lifted)
        withAnimation(outcome == .confirm ? settleAnimation : springBack) { track = settled }
        if outcome == .confirm { fireConfirmAction() }
    }

    func confirm() {
        guard isInteractive else { return }
        var settled = track
        guard settled.confirm() == .confirm else { return }
        withAnimation(settleAnimation) { track = settled }
        fireConfirmAction()
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
