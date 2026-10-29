import MetalKit
import UIKit

// MARK: - Field

/// The whole field, as tall as the layout says, holding the Metal view that is at most one screen
/// tall and pinned to whichever part of the field is on screen.
///
/// Drawing the whole of a five hundred coin grid every frame was most of what made it slow: the
/// GPU filled five screens of pixels to show one. The pin is done here in UIKit, by watching the
/// enclosing scroll view's content offset, because a SwiftUI geometry reader reports the field's
/// position once and not again while the page scrolls.
final class CoinageFieldView: UIView {
    let surface: CoinageMetalView

    /// How far down the field the surface sits, and then the part of it clear of the bars over
    /// the top and bottom of the scroll view, as a top and a height: where the band may go.
    var onWindow: ((CGFloat, CGFloat, CGFloat) -> Void)?

    private var pinned: CGFloat = 0
    private var lastClear: (CGFloat, CGFloat) = (0, 0)
    private var offsetWatch: NSKeyValueObservation?

    init(surface: CoinageMetalView) {
        self.surface = surface
        super.init(frame: .zero)
        addSubview(surface)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        offsetWatch = nil

        guard window != nil, let scrollView = enclosingScrollView else { return }

        offsetWatch = scrollView.observe(\.contentOffset, options: [.initial, .new]) { [weak self] _, _ in
            self?.follow()
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        follow()
    }

    /// The most the surface is: the scroll view's visible height, so a frame costs the same
    /// however tall the field.
    private var surfaceHeight: CGFloat {
        let visible = enclosingScrollView?.bounds.height ?? window?.windowScene?.screen.bounds.height ?? 800

        return min(bounds.height, visible)
    }

    private func follow() {
        let reach = max(bounds.height - surfaceHeight, 0)
        var top: CGFloat = 0
        var clearTop: CGFloat = 0
        var clearBottom = bounds.height

        if let scrollView = enclosingScrollView {
            // Against the scroll view's own bounds, not the area under its insets: the bars over
            // the top and bottom are translucent, and what lies under them has to be drawn too.
            let fieldTop = scrollView.convert(bounds, from: self).minY
            top = min(max(scrollView.bounds.minY - fieldTop, 0), reach)

            // What is really on screen and clear of the bars, in the field's own coordinates,
            // which is where the band can go. The field may start below the top of the screen
            // or end above its bottom, so both ends are clipped to it.
            let insets = scrollView.adjustedContentInset
            clearTop = max(scrollView.bounds.minY + insets.top - fieldTop, 0)
            clearBottom = min(scrollView.bounds.maxY - insets.bottom - fieldTop, bounds.height)
        }

        let frame = CGRect(x: 0, y: top, width: bounds.width, height: surfaceHeight)

        if surface.frame != frame {
            surface.frame = frame
        }

        let clear = (clearTop, max(clearBottom - clearTop, 0))

        if top != pinned || clear != lastClear {
            pinned = top
            lastClear = clear
            onWindow?(top, clear.0, clear.1)
        }
    }

    private var enclosingScrollView: UIScrollView? {
        var ancestor = superview

        while let current = ancestor {
            if let scrollView = current as? UIScrollView { return scrollView }

            ancestor = current.superview
        }

        return nil
    }
}

// MARK: - Touches

/// The Metal view with the touches the coins answer to: a tap, and a flick down.
///
/// UIKit recognisers rather than SwiftUI gestures, because the view sits in the wallet's scroll
/// view. A SwiftUI drag there is cancelled the moment the scroll recognises, and a high-priority
/// one takes every touch, which would leave a tall grid impossible to scroll through. A swipe
/// recogniser only ever fires on a flick, and the scroll view is asked to wait for it to fail
/// first: a slow drag on the coins still scrolls the page, and a flick on them is theirs.
final class CoinageMetalView: MTKView {
    var onTap: ((CGPoint) -> Void)?
    var onSwipe: ((CoinageCoinsView.Swipe) -> Void)?

    private let swipes: [UISwipeGestureRecognizer]

    /// Whether a flick is listened for at all. Only on the strip: the scroll view waits for the
    /// flick to fail before it scrolls, which on a tall grid read as the page refusing to move.
    var listensForSwipes = true {
        didSet { swipes.forEach { $0.isEnabled = listensForSwipes } }
    }

    override init(frame: CGRect, device: MTLDevice?) {
        // Only a flick down, which unfurls the strip. A flick up used to gather the grid back and
        // collided with scrolling up through it; the grid closes on a tap on empty ground instead.
        swipes = [UISwipeGestureRecognizer.Direction.down].map { direction in
            let swipe = UISwipeGestureRecognizer()
            swipe.direction = direction

            return swipe
        }

        super.init(frame: frame, device: device)

        isUserInteractionEnabled = true
        addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped)))

        for swipe in swipes {
            swipe.addTarget(self, action: #selector(swiped))
            swipe.delegate = self
            addGestureRecognizer(swipe)
        }
    }

    @available(*, unavailable)
    required init(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// True while a frame is being drawn inside the layout pass that resized the view.
    private(set) var isDrawingWithinTransaction = false
    private var presentedSize: CGSize = .zero

    /// Draws the first frame at a new size inside the transaction that set it.
    ///
    /// When a Metal layer's drawable changes size, Core Animation drops what the layer was
    /// showing and shows nothing until a frame is presented, which from the display link is a
    /// frame later: one blank frame every time the card gave the surface back. Presenting here,
    /// with the layer told to wait for the transaction, puts the new frame and the new bounds on
    /// screen together.
    override func layoutSubviews() {
        super.layoutSubviews()

        guard window != nil, drawableSize != presentedSize, drawableSize.width > 0 else { return }

        presentedSize = drawableSize

        guard let metalLayer = layer as? CAMetalLayer else { return }

        metalLayer.presentsWithTransaction = true
        isDrawingWithinTransaction = true
        draw()
        isDrawingWithinTransaction = false
        metalLayer.presentsWithTransaction = false
    }

    @objc private func tapped(_ recognizer: UITapGestureRecognizer) {
        onTap?(recognizer.location(in: self))
    }

    @objc private func swiped(_: UISwipeGestureRecognizer) {
        onSwipe?(.downward)
    }
}

extension CoinageMetalView: UIGestureRecognizerDelegate {
    /// The scroll view's pan waits for a flick on the coins to fail, and only for a touch that
    /// is on them. Asking the pan outright, with `require(toFail:)`, held every touch on the
    /// page until the swipe gave up, and a quick tap on a switch further down was dropped.
    func gestureRecognizer(
        _: UIGestureRecognizer,
        shouldBeRequiredToFailBy other: UIGestureRecognizer
    ) -> Bool {
        other is UIPanGestureRecognizer && other.view is UIScrollView
    }
}
