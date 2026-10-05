import UIKit
import DesignSystem

/// Hosts either a content configuration or a child controller's view; both size themselves and
/// are measured the same way. A controller cannot be wrapped in a `UIContentView` without
/// losing its appearance callbacks, so the two modes exist side by side and are exclusive.
public final class DSTabBarContentPanelView: UIView {
    public private(set) var isOpen = false

    private let container = UIView()
    private var contentView: (UIView & UIContentView)?
    private var contentReuseIdentifier: String?
    private var hostedView: UIView?

    override public init(frame: CGRect) {
        super.init(frame: frame)

        // Clipping cut the hosted content's soft glass edge into a seam along the panel's bottom.
        container.clipsToBounds = false
        container.alpha = 0
        // A closed panel keeps the open panel's frame and would otherwise
        // hit-test as itself, swallowing touches meant for whatever sits
        // behind it.
        isUserInteractionEnabled = false
        addSubview(container)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func setConfiguration(_ configuration: (any HashableContentConfiguration)?) {
        guard let configuration else {
            clearContentView()
            return
        }

        clearHostedView()

        if let contentView,
           contentReuseIdentifier == configuration.defaultReuseIdentifier {
            contentView.configuration = configuration
            contentView.invalidateIntrinsicContentSize()
            return
        }

        contentView?.removeFromSuperview()

        let newContentView = configuration.makeContentView()
        container.addSubview(newContentView)

        contentView = newContentView
        contentReuseIdentifier = configuration.defaultReuseIdentifier
        setNeedsLayout()
    }

    /// The hosted view's own constraints decide the panel height. The bottom pin yields so a view
    /// with a required aspect constraint keeps its shape while the container animates to fit it.
    public func setHostedView(_ view: UIView?) {
        guard let view else {
            clearHostedView()
            return
        }

        clearContentView()

        guard hostedView !== view else {
            setNeedsLayout()
            return
        }

        hostedView?.removeFromSuperview()
        hostedView = view
        view.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(view)

        let bottom = view.bottomAnchor.constraint(equalTo: container.bottomAnchor)
        bottom.priority = .defaultHigh

        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            bottom
        ])

        // Settle the hosted view at its final size w/o animation
        setNeedsLayout()
        layoutIfNeeded()
    }

    public func preferredHeight(availableHeight: CGFloat) -> CGFloat {
        guard let measuredView = hostedView ?? contentView, bounds.width > 0 else {
            return DSTabBarMetrics.capsuleHeight
        }

        // A SwiftUI backed configuration view still reports the size of the content it laid out
        // last, so a measurement taken right after a configuration change returns the previous
        // panel height — and layout passes stop once the open animation ends, so nothing corrects
        // it. Settling the view at the width it is about to be measured at avoids that. Only the
        // configuration view is frame driven; the hosted view keeps sizing itself from its own
        // constraints.
        if measuredView === contentView {
            measuredView.frame.size.width = bounds.width
        }
        measuredView.layoutIfNeeded()

        let measuredSize = measuredView.systemLayoutSizeFitting(
            CGSize(width: bounds.width, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .fittingSizeLevel
        )

        return DSTabBarPanelLayout.panelHeight(
            contentHeight: measuredSize.height,
            availableHeight: availableHeight
        )
    }

    /// Adds the panel's open/close animations to `animator` so they stay in lockstep with the
    /// container resize the caller drives; applies them immediately when `animator` is nil.
    public func setOpen(_ open: Bool, animator: UIViewPropertyAnimator?) {
        guard open != isOpen else {
            return
        }
        isOpen = open

        // Not `isHidden`: that cannot animate, so it would have to wait on the animator and
        // then race a reopen during the fade. Interaction can flip immediately instead.
        isUserInteractionEnabled = open

        // A drag that ended in a close leaves the content clipped behind it.
        if open {
            setContentClipped(false)
        }

        let apply = { [self] in
            container.alpha = open ? 1 : 0
        }

        guard let animator else {
            apply()
            return
        }

        animator.addAnimations(apply)
    }

    /// Clips the hosted content while an interactive drag shrinks the panel. The content keeps
    /// its own height, so without this it spills past the panel's bottom edge and shows through
    /// the capsule. Clipping stays off at rest, where it would cut the content's soft glass edge
    /// into a seam along the bottom.
    public func setContentClipped(_ clipped: Bool) {
        container.clipsToBounds = clipped
    }

    override public func layoutSubviews() {
        super.layoutSubviews()

        container.frame = bounds
        contentView?.frame = container.bounds
    }
}

private extension DSTabBarContentPanelView {
    func clearContentView() {
        contentView?.removeFromSuperview()
        contentView = nil
        contentReuseIdentifier = nil
    }

    func clearHostedView() {
        hostedView?.removeFromSuperview()
        hostedView = nil
    }
}
