import CoreGraphics

/// Where a slide-to-confirm control stands, and the one confirmation it may
/// fire. Kept apart from the view so the rules are testable without touches.
///
/// Progress runs from 0 (at rest) to 1 (slid to the end), along the reading
/// direction. Once the control confirms, or the product draws it as loading or
/// confirmed, it is held at the end and accepts no further slide until the
/// product puts it back.
struct SlideToConfirmTrack: Equatable {
    /// How far the thumb must be released past for the slide to count, as on Android.
    static let confirmThreshold: CGFloat = 0.9

    private(set) var progress: CGFloat
    private(set) var isHeldAtEnd: Bool
    private var dragStartProgress: CGFloat?

    init(heldAtEnd: Bool = false) {
        progress = heldAtEnd ? 1 : 0
        isHeldAtEnd = heldAtEnd
    }

    var isDragging: Bool {
        dragStartProgress != nil
    }

    /// Follows the finger. `translation` is measured from where the drag began,
    /// along the reading direction, and `travel` is the distance a full slide covers.
    mutating func drag(translation: CGFloat, travel: CGFloat) {
        guard !isHeldAtEnd, travel > 0 else { return }
        let start = dragStartProgress ?? progress
        dragStartProgress = start
        progress = min(max(start + translation / travel, 0), 1)
    }

    /// Ends a drag. A finger lifted past the threshold confirms; anything else,
    /// including a drag the system cancelled, springs back to the start.
    mutating func release(lifted: Bool) -> Outcome {
        guard dragStartProgress != nil else { return .none }
        dragStartProgress = nil
        guard !isHeldAtEnd else { return .none }

        if lifted, progress >= Self.confirmThreshold {
            return confirm()
        }
        progress = 0
        return .springBack
    }

    /// Confirms without a drag, as the accessibility action does.
    mutating func confirm() -> Outcome {
        guard !isHeldAtEnd else { return .none }
        isHeldAtEnd = true
        progress = 1
        return .confirm
    }

    /// Follows the product's state. Loading or confirmed holds the control at
    /// the end; a tree that is neither puts a control the user slid back at the start.
    mutating func follow(productHoldsAtEnd: Bool) {
        dragStartProgress = nil
        if productHoldsAtEnd {
            isHeldAtEnd = true
            progress = 1
        } else if isHeldAtEnd {
            isHeldAtEnd = false
            progress = 0
        }
    }
}

extension SlideToConfirmTrack {
    enum Outcome: Equatable {
        /// Fire the confirm action, once.
        case confirm
        /// Animate back to the start.
        case springBack
        case none
    }
}
