import UIKit

open class PlainBaseTableViewCell<C: UIView>: UITableViewCell {
    public let contentDisplayView = C()

    public var contentInsets = UIEdgeInsets.zero {
        didSet {
            if oldValue != contentInsets {
                updateLayout()
            }
        }
    }

    override public init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
        super.init(style: style, reuseIdentifier: reuseIdentifier)

        setupStyle()
        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func setupStyle() {}

    public func setupLayout() {
        contentView.addSubview(contentDisplayView)

        contentDisplayView.snp.makeConstraints { make in
            make.edges.equalToSuperview().inset(contentInsets)
        }
    }

    private func updateLayout() {
        contentDisplayView.snp.updateConstraints { make in
            make.edges.equalToSuperview().inset(contentInsets)
        }
    }
}
