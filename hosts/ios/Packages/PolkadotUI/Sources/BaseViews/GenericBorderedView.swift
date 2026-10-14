import UIKit
public import UIKit_iOS

open class GenericBorderedView<TContentView: UIView>: UIView {
    public var contentView: TContentView = .init()

    public let backgroundView = RoundedView()

    public var contentInsets = UIEdgeInsets(top: 1.0, left: 8.0, bottom: 2.0, right: 8.0) {
        didSet {
            if oldValue != contentInsets {
                updateLayout()
            }
        }
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func updateLayout() {
        contentView.snp.updateConstraints { make in
            make.edges.equalToSuperview().inset(contentInsets)
        }
    }

    private func setupLayout() {
        addSubview(backgroundView)
        backgroundView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        backgroundView.addSubview(contentView)
        contentView.snp.makeConstraints { make in
            make.edges.equalToSuperview().inset(contentInsets)
        }
    }
}
