import DesignSystem
import UIKit

internal import SnapKit

/// Signals that the panel can be pulled down. Displays a centered horizontal pill.
public final class DSPanelGrabberView: UIView {
    private enum Constants {
        static let stripHeight: CGFloat = 20
        static let pillWidth: CGFloat = 36
        static let pillHeight: CGFloat = 4
        static let touchHeight: CGFloat = 44
    }

    private let pill: UIView = {
        let view = UIView()
        view.backgroundColor = .fgTertiary
        view.layer.cornerRadius = Constants.pillHeight / 2
        view.layer.masksToBounds = true
        return view
    }()

    public var onDragChanged: ((CGFloat) -> Void)?
    public var onDragEnded: ((CGFloat) -> Void)?

    override public init(frame: CGRect) {
        super.init(frame: frame)

        addSubview(pill)

        pill.snp.makeConstraints { make in
            make.width.equalTo(Constants.pillWidth)
            make.height.equalTo(Constants.pillHeight)
            make.center.equalToSuperview()
        }

        let panGesture = UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:)))
        addGestureRecognizer(panGesture)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public var intrinsicContentSize: CGSize {
        CGSize(width: UIView.noIntrinsicMetric, height: Constants.stripHeight)
    }

    /// Extends the touch target downward to 44pt for reliable grabbability.
    ///
    /// The rendered strip stays 20pt (`intrinsicContentSize`) so the panel's content
    /// doesn't shift, but the drag target spans 44pt total — extending 24pt below the
    /// pill. This extra area overlaps the results list (drag-only by design).
    override public func point(inside point: CGPoint, with _: UIEvent?) -> Bool {
        var touchRect = bounds
        touchRect.size.height = Constants.touchHeight
        return touchRect.contains(point)
    }
}

private extension DSPanelGrabberView {
    @objc func handlePan(_ recognizer: UIPanGestureRecognizer) {
        guard let superview else { return }
        let translation = recognizer.translation(in: superview).y

        switch recognizer.state {
        case .began,
             .changed:
            onDragChanged?(translation)
        case .ended:
            onDragEnded?(translation)
        case .cancelled,
             .failed:
            onDragEnded?(0)
        default:
            break
        }
    }
}
