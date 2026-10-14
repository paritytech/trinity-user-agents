import UIKit

public class WidthResizableContainer<C: UIView>: UIView {
    public let contentView = C()

    public convenience init() {
        self.init(frame: .zero)
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupLayout() {
        addSubview(contentView)

        contentView.snp.makeConstraints { make in
            make.centerX.equalToSuperview()
            make.leading.greaterThanOrEqualToSuperview()
            make.trailing.lessThanOrEqualToSuperview()
            make.height.equalToSuperview()
        }

        contentView.setContentHuggingPriority(.defaultLow, for: .horizontal)
    }
}
