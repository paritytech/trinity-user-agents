import UIKit
public import UIKit_iOS

public struct RowViewStyle {
    public let separatorColor: UIColor
    public let separatorWidth: CGFloat
    public let highlightedColor: UIColor

    public init(separatorColor: UIColor, separatorWidth: CGFloat, highlightedColor: UIColor) {
        self.separatorColor = separatorColor
        self.separatorWidth = separatorWidth
        self.highlightedColor = highlightedColor
    }
}

public extension RowViewStyle {
    static var defaultStyle: RowViewStyle {
        .init(
            separatorColor: UIColor(resource: .separatorDark),
            separatorWidth: 1,
            highlightedColor: .clear
        )
    }
}

open class RowView<T: UIView>: BackgroundedContentControl {
    public var preferredHeight: CGFloat? {
        didSet {
            invalidateLayout()
        }
    }

    public let borderView: BorderedContainerView = .create { view in
        view.borderType = [.bottom]
    }

    private var calculatedHeight: CGFloat = 0.0
    private var calculatedWidth: CGFloat = 0.0

    public let style: RowViewStyle

    public var rowContentView: T! { contentView as? T }

    public var roundedBackgroundView: RoundedView! { backgroundView as? RoundedView }

    public var hasInteractableContent: Bool = false {
        didSet {
            updateContentInteraction()
        }
    }

    public var extendsBorders: Bool = false {
        didSet {
            setNeedsLayout()
        }
    }

    public init(contentView: T? = nil, preferredHeight: CGFloat? = nil, style: RowViewStyle) {
        self.preferredHeight = preferredHeight
        self.style = style

        super.init(frame: .zero)

        self.contentView = contentView

        setupLayout()
        applyStyle()
    }

    override public init(frame: CGRect) {
        style = .init(separatorColor: .clear, separatorWidth: 0, highlightedColor: .clear)

        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func layoutSubviews() {
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

        if extendsBorders {
            borderView.frame = CGRect(
                x: bounds.minX,
                y: bounds.minY,
                width: bounds.width,
                height: bounds.height
            )
        } else {
            borderView.frame = CGRect(
                x: bounds.minX + contentInsets.left,
                y: bounds.minY,
                width: width,
                height: bounds.height
            )
        }
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
        contentInsets = UIEdgeInsets.zero

        let shapeView = RoundedView()
        shapeView.shadowOpacity = 0.0
        shapeView.strokeWidth = 0.0
        shapeView.isUserInteractionEnabled = false
        shapeView.fillColor = .clear
        shapeView.cornerRadius = 0.0
        backgroundView = shapeView

        borderView.isUserInteractionEnabled = false
        shapeView.addSubview(borderView)

        if contentView == nil {
            contentView = T()
        }

        contentView?.autoresizingMask = [.flexibleWidth, .flexibleHeight]

        updateContentInteraction()
    }

    private func applyStyle() {
        borderView.strokeColor = style.separatorColor
        borderView.strokeWidth = style.separatorWidth
    }

    private func updateContentSizeForWidth(_ width: CGFloat) {
        calculatedWidth = width

        let size = rowContentView.systemLayoutSizeFitting(
            CGSize(width: width, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .defaultLow
        )

        calculatedHeight = size.height

        invalidateIntrinsicContentSize()
    }

    private func updateContentInteraction() {
        contentView?.isUserInteractionEnabled = hasInteractableContent
        let color = hasInteractableContent ? .clear : style.highlightedColor
        roundedBackgroundView?.highlightedFillColor = color
    }
}

extension RowView: StackTableViewCellProtocol {}

class FramedRowView<T: UIView>: RowView<T> {
    override func layoutSubviews() {
        super.layoutSubviews()

        let contentFrame = rowContentView.frame

        let height = max(bounds.height - contentInsets.top - contentInsets.bottom, 0)

        rowContentView.frame = CGRect(
            origin: contentFrame.origin,
            size: CGSize(width: contentFrame.size.width, height: height)
        )
    }
}
