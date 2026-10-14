import UIKit
public import UIKit_iOS

open class ControlView<B: UIView, T: UIView>: BackgroundedContentControl {
    public var preferredHeight: CGFloat? {
        didSet {
            invalidateLayout()
        }
    }

    private var calculatedHeight: CGFloat = 0.0
    private var calculatedWidth: CGFloat = 0.0

    public var controlBackgroundView: B! { backgroundView as? B }

    public var controlContentView: T! { contentView as? T }

    public init(backgroundView: B? = nil, contentView: T? = nil, preferredHeight: CGFloat? = nil) {
        self.preferredHeight = preferredHeight

        super.init(frame: .zero)

        self.backgroundView = backgroundView
        self.contentView = contentView

        setupLayout()
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override open func layoutSubviews() {
        let contentHeight: CGFloat

        let width = max(bounds.width - contentInsets.left - contentInsets.right, 0)

        if let preferredHeight {
            contentHeight = preferredHeight - contentInsets.top - contentInsets.bottom
        } else {
            if abs(calculatedWidth - width) > CGFloat.leastNormalMagnitude {
                updateContentSizeForWidth(width)
            }

            contentHeight = calculatedHeight
        }

        backgroundView?.frame = bounds

        contentView?.frame = CGRect(
            x: bounds.minX + contentInsets.left,
            y: bounds.minY + contentInsets.top,
            width: width,
            height: contentHeight
        )
    }

    override open var intrinsicContentSize: CGSize {
        let height: CGFloat =
            if let preferredHeight {
                preferredHeight
            } else {
                calculatedHeight + contentInsets.bottom + contentInsets.top
            }

        return CGSize(
            width: UIView.noIntrinsicMetric,
            height: height
        )
    }

    private func setupLayout() {
        contentInsets = .zero

        if backgroundView == nil {
            backgroundView = B()
        }

        if contentView == nil {
            contentView = T()
        }

        backgroundView?.isUserInteractionEnabled = false
        contentView?.isUserInteractionEnabled = false
        contentView?.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    }

    private func updateContentSizeForWidth(_ width: CGFloat) {
        calculatedWidth = width

        let size = controlContentView.systemLayoutSizeFitting(
            CGSize(width: width, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .defaultLow
        )

        calculatedHeight = size.height

        invalidateIntrinsicContentSize()
    }
}
