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
        SlideToConfirmFace(props: props, progress: track.progress, isHinting: isInteractive && !track.isDragging)
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
        max(width - SlideToConfirmMetrics.travelEndInset, 0)
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
    /// What a full slide leaves uncovered at the end: a thumb's width, or the last of a peeled face.
    static let travelEndInset: CGFloat = 56
    static let disabledOpacity: CGFloat = 0.4
    /// The label is gone once the thumb has travelled this share of the track.
    static let labelGoneAt: CGFloat = 0.6
    static let revealHeight: CGFloat = 96
}
