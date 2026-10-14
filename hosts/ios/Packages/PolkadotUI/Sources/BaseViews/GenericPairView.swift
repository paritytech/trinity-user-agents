import UIKit
internal import SnapKit

open class GenericPairValueView<FView: UIView, SView: UIView>: UIView {
    public let fView = FView()
    public let sView = SView()

    public var spacing: CGFloat {
        get {
            stackView.spacing
        }

        set {
            stackView.spacing = newValue
        }
    }

    public let stackView: UIStackView = {
        let view = UIStackView()
        view.axis = .vertical
        return view
    }()

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    public func makeVertical() {
        stackView.axis = .vertical

        setNeedsLayout()
    }

    public func makeHorizontal() {
        stackView.axis = .horizontal

        setNeedsLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func setVerticalAndSpacing(_ spacing: CGFloat) {
        stackView.axis = .vertical
        stackView.spacing = spacing
    }

    public func setHorizontalAndSpacing(_ spacing: CGFloat) {
        stackView.axis = .horizontal
        stackView.spacing = spacing
    }

    private func setupLayout() {
        addSubview(stackView)
        stackView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        stackView.addArrangedSubview(fView)
        stackView.addArrangedSubview(sView)
    }
}
