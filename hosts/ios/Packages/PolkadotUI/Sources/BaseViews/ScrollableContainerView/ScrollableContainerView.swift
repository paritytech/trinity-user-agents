import UIKit

public final class ScrollableContainerView: UIView {
    public let scrollView = UIScrollView()
    public let stackView = UIStackView()

    private var scrollBottom: NSLayoutConstraint!
    private var scrollTop: NSLayoutConstraint!

    public var respectsSafeArea: Bool {
        didSet {
            updateTopConstraint()
        }
    }

    public var scrollBottomOffset: CGFloat {
        get {
            -scrollBottom.constant
        }

        set {
            scrollBottom.constant = -newValue

            if superview != nil {
                setNeedsLayout()
            }
        }
    }

    public init(axis: NSLayoutConstraint.Axis, respectsSafeArea: Bool = true) {
        self.respectsSafeArea = respectsSafeArea

        super.init(frame: .zero)

        configureScrollView()
        configureStackView(with: axis)
    }

    override public init(frame: CGRect) {
        respectsSafeArea = true

        super.init(frame: frame)

        configureScrollView()
        configureStackView(with: .vertical)
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func updateTopConstraint() {
        scrollTop.isActive = false
        scrollView.removeConstraint(scrollTop)

        if respectsSafeArea {
            scrollTop = scrollView.topAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor)
        } else {
            scrollTop = scrollView.topAnchor.constraint(equalTo: topAnchor)
        }

        scrollTop.isActive = true
    }

    private func configureScrollView() {
        scrollView.backgroundColor = .clear
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(scrollView)

        scrollView.leadingAnchor.constraint(equalTo: leadingAnchor).isActive = true
        scrollView.rightAnchor.constraint(equalTo: rightAnchor).isActive = true

        if respectsSafeArea {
            scrollTop = scrollView.topAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor)
        } else {
            scrollTop = scrollView.topAnchor.constraint(equalTo: topAnchor)
        }

        scrollTop.isActive = true

        let bottomConstraint = scrollView.bottomAnchor.constraint(equalTo: bottomAnchor)
        bottomConstraint.isActive = true

        scrollBottom = bottomConstraint
    }

    private func configureStackView(with axis: NSLayoutConstraint.Axis) {
        stackView.backgroundColor = .clear
        stackView.axis = axis
        stackView.alignment = .center
        stackView.distribution = .fill
        stackView.translatesAutoresizingMaskIntoConstraints = false

        scrollView.addSubview(stackView)

        switch axis {
        case .horizontal:
            stackView.heightAnchor.constraint(equalTo: heightAnchor).isActive = true
        case .vertical:
            stackView.widthAnchor.constraint(equalTo: widthAnchor).isActive = true
        @unknown default:
            break
        }

        stackView.leadingAnchor.constraint(equalTo: scrollView.leadingAnchor).isActive = true
        stackView.rightAnchor.constraint(equalTo: scrollView.rightAnchor).isActive = true
        stackView.topAnchor.constraint(equalTo: scrollView.topAnchor).isActive = true
        stackView.bottomAnchor.constraint(equalTo: scrollView.bottomAnchor).isActive = true
    }
}
