import UIKit
internal import SnapKit
internal import UIKit_iOS

open class ScrollableContainerLayoutView: UIView {
    public var layoutInsets: UIEdgeInsets {
        get {
            containerView.stackView.layoutMargins
        }

        set {
            containerView.stackView.layoutMargins = newValue
        }
    }

    public let containerView: ScrollableContainerView = {
        let view = ScrollableContainerView(axis: .vertical, respectsSafeArea: true)
        view.stackView.layoutMargins = UIEdgeInsets(
            top: 0,
            left: UIConstants.horizontalInsetShort,
            bottom: 0.0,
            right: UIConstants.horizontalInsetShort
        )

        view.stackView.isLayoutMarginsRelativeArrangement = true
        view.stackView.alignment = .fill
        return view
    }()

    public var stackView: UIStackView { containerView.stackView }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupStyle()
        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    open func setupStyle() {}

    open func setupLayout() {
        addSubview(containerView)
        containerView.snp.makeConstraints { make in
            make.top.leading.trailing.bottom.equalToSuperview()
        }
    }

    public func addArrangedSubview(_ view: UIView, spacingAfter value: CGFloat = 0) {
        stackView.addArrangedSubview(view)

        if value > 0 {
            stackView.setCustomSpacing(value, after: view)
        }
    }

    public func insertArrangedSubview(_ view: UIView, after oldView: UIView, spacingAfter value: CGFloat = 0) {
        stackView.insertArranged(view: view, after: oldView)

        if value > 0 {
            stackView.setCustomSpacing(value, after: view)
        }
    }

    public func insertArrangedSubview(_ view: UIView, at index: Int, spacingAfter value: CGFloat = 0) {
        stackView.insertSubview(view, at: index)

        if value > 0 {
            stackView.setCustomSpacing(value, after: view)
        }
    }
}
