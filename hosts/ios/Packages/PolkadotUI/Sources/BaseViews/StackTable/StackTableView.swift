import UIKit
public import UIKit_iOS

public protocol StackTableViewCellProtocol: UIView {
    var borderView: BorderedContainerView { get }
    var contentInsets: UIEdgeInsets { get set }
    var preferredHeight: CGFloat? { get set }
    var roundedBackgroundView: RoundedView! { get }
}

public struct StackTableViewStyle {
    public let fillColor: UIColor
    public let cornerRadius: CGFloat

    public init(fillColor: UIColor, cornerRadius: CGFloat) {
        self.fillColor = fillColor
        self.cornerRadius = cornerRadius
    }
}

public final class StackTableView: RoundedView {
    public let stackView: UIStackView = {
        let view = UIStackView()
        view.axis = .vertical
        view.alignment = .fill
        return view
    }()

    public var hasSeparators: Bool = true {
        didSet {
            if oldValue != hasSeparators {
                updateLayout()
            }
        }
    }

    public var contentInsets = UIEdgeInsets(top: 0.0, left: 16.0, bottom: 0.0, right: 16.0) {
        didSet {
            if oldValue != contentInsets {
                updateLayout()
            }
        }
    }

    public var cellHeight: CGFloat? {
        didSet {
            if oldValue != cellHeight {
                updateLayout()
            }
        }
    }

    private var customHeights: [Int: CGFloat] = [:]
    private var showsSeparatorStore: [Int: Bool] = [:]

    public init(frame: CGRect, style: StackTableViewStyle) {
        super.init(frame: frame)

        apply(style: style)
        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func addArrangedSubview(_ view: StackTableViewCellProtocol) {
        stackView.addArrangedSubview(view)
        updateLayout()
    }

    public func insertArrangedSubview(_ view: StackTableViewCellProtocol, at index: Int) {
        stackView.insertArrangedSubview(view, at: index)
        updateLayout()
    }

    public func insertArranged(view: StackTableViewCellProtocol, after subview: UIView) {
        stackView.insertArranged(view: view, after: subview)
        updateLayout()
    }

    public func insertArranged(view: StackTableViewCellProtocol, before subview: UIView) {
        stackView.insertArranged(view: view, before: subview)
        updateLayout()
    }

    public func clear() {
        for cell in stackView.arrangedSubviews {
            cell.removeFromSuperview()
        }
    }

    public func setCustomHeight(_ height: CGFloat?, at index: Int) {
        customHeights[index] = height

        updateLayout()
    }

    public func setShowsSeparator(_ value: Bool, at index: Int) {
        showsSeparatorStore[index] = value

        updateLayout()
    }

    public func updateLayout() {
        let views = stackView.arrangedSubviews

        for (index, view) in views.enumerated() {
            guard let rowView = view as? StackTableViewCellProtocol else {
                continue
            }

            if let fixedHeight = getFixedCellHeight(at: index) {
                rowView.preferredHeight = fixedHeight
            }

            rowView.borderView.borderType = shouldShowSeparator(at: index) ? [.bottom] : []
            rowView.roundedBackgroundView.cornerRadius = 0.0
            rowView.roundedBackgroundView.roundingCorners = []
            rowView.contentInsets = UIEdgeInsets(
                top: 0.0,
                left: contentInsets.left,
                bottom: 0.0,
                right: contentInsets.right
            )
        }

        guard
            let lastView = views.last as? StackTableViewCellProtocol,
            let firstView = views.first as? StackTableViewCellProtocol
        else {
            return
        }

        lastView.borderView.borderType = []

        var lastViewInsets = lastView.contentInsets
        lastViewInsets.bottom = contentInsets.bottom
        lastView.contentInsets = lastViewInsets

        // cell selection doesn't take insets into account for dynamically resized cells
        if let lastViewHeight = getFixedCellHeight(at: views.count - 1) {
            lastView.preferredHeight = lastViewHeight + contentInsets.bottom
        }

        lastView.roundedBackgroundView.cornerRadius = cornerRadius

        var lastRoundingCorners = lastView.roundedBackgroundView.roundingCorners
        lastRoundingCorners = lastRoundingCorners.union([.bottomLeft, .bottomRight])
        lastView.roundedBackgroundView.roundingCorners = lastRoundingCorners

        firstView.roundedBackgroundView.cornerRadius = cornerRadius

        var firstViewInsets = firstView.contentInsets
        firstViewInsets.top = contentInsets.top
        firstView.contentInsets = firstViewInsets

        // cell selection doesn't take insets into account for dynamically resized cells
        if let firstViewHeight = getFixedCellHeight(at: 0) {
            firstView.preferredHeight = firstViewHeight + contentInsets.top
        }

        var firstRoundingCorners = firstView.roundedBackgroundView.roundingCorners
        firstRoundingCorners = firstRoundingCorners.union([.topLeft, .topRight])
        firstView.roundedBackgroundView.roundingCorners = firstRoundingCorners

        invalidateIntrinsicContentSize()
        setNeedsLayout()
    }

    private func getFixedCellHeight(at index: Int) -> CGFloat? {
        customHeights[index] ?? cellHeight
    }

    private func shouldShowSeparator(at index: Int) -> Bool {
        showsSeparatorStore[index] ?? hasSeparators
    }

    private func apply(style: StackTableViewStyle) {
        shadowOpacity = 0
        strokeWidth = 0

        fillColor = style.fillColor
        cornerRadius = style.cornerRadius
    }

    private func setupLayout() {
        addSubview(stackView)
        stackView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }
}
