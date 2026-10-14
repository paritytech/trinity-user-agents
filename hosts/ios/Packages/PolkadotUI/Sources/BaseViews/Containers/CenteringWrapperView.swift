import UIKit

public class CenteringWrapperView<ContentView: UIView>: UIView {
    public let contentView = ContentView(frame: .zero)

    public var spacerMultiplier = CGFloat(1) {
        didSet { updateLayout() }
    }

    public var minimumSpacerSize = CGFloat(40) {
        didSet { updateLayout() }
    }

    private let fSpacerView: UIView = create {
        $0.isHidden = true
    }

    private let sSpacerView: UIView = create {
        $0.isHidden = true
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("not implemented")
    }
}

// MARK: - Private

private extension CenteringWrapperView {
    func setupLayout() {
        addSubview(contentView)
        contentView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview()
        }

        addSubview(fSpacerView)
        fSpacerView.snp.makeConstraints {
            $0.top.leading.trailing.equalToSuperview()
            $0.bottom.equalTo(contentView.snp.top)
        }

        addSubview(sSpacerView)
        sSpacerView.snp.makeConstraints {
            $0.bottom.leading.trailing.equalToSuperview()
            $0.top.equalTo(contentView.snp.bottom)
            $0.height.equalTo(fSpacerView.snp.height).multipliedBy(spacerMultiplier)
            $0.height.greaterThanOrEqualTo(minimumSpacerSize)
        }
    }

    func updateLayout() {
        sSpacerView.snp.remakeConstraints {
            $0.bottom.leading.trailing.equalToSuperview()
            $0.top.equalTo(contentView.snp.bottom)
            $0.height.equalTo(fSpacerView.snp.height).multipliedBy(spacerMultiplier)
            $0.height.greaterThanOrEqualTo(minimumSpacerSize)
        }
    }
}
