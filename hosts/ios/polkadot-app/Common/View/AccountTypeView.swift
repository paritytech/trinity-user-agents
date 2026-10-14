import UIKit
import SnapKit
import PolkadotUI
import DesignSystem

final class AccountTypeView: GenericBorderedView<UIView> {
    var preferredSize: CGFloat = 56 {
        didSet {
            invalidateIntrinsicContentSize()

            backgroundView.cornerRadius = preferredSize / 2
        }
    }

    private var iconView: UIView?

    convenience init() {
        self.init(frame: .zero)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        configure()
    }

    override var intrinsicContentSize: CGSize {
        CGSize(width: preferredSize, height: preferredSize)
    }

    func bind(account: SearchAccountViewModel.AccountType) {
        switch account {
        case let .username(string, _) where iconView is UILabel:
            (iconView as? UILabel)?.text = string.first.map { String($0).uppercased() } ?? ""
        case let .username(string, _):
            iconView?.removeFromSuperview()

            let label = Label()
            label.textColor = .fgTertiary
            label.textAlignment = .center
            label.typography = .headlineSmall
            label.adjustsFontSizeToFitWidth = true
            label.minimumScaleFactor = 0.5
            label.text = string.first.map { String($0).uppercased() } ?? ""
            iconView = label
        case .accountAddress where iconView is UIImageView:
            break
        case .accountAddress:
            iconView?.removeFromSuperview()
            let imageView = UIImageView(image: .iconAccount)
            imageView.contentMode = .scaleAspectFit
            iconView = imageView
        }

        guard
            let iconView,
            iconView.superview == nil
        else {
            return
        }
        contentView.addSubview(iconView)
        iconView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    private func configure() {
        backgroundView.applyBackgroundStyle(
            .bgActionSecondary,
            cornerRadius: preferredSize / 2
        )
    }
}
