import PolkadotUI
import UIKit

/// The open Purse's header title, which fades as the card shrinks into the
/// header in its place.
final class PurseTitleView: UIView {
    private let titleLabel: PolkadotUI.Label = .create { view in
        view.typography = .titleLarge
        view.textColor = .fgPrimary
        view.textAlignment = .center
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        titleLabel.text = String(localized: .walletMainPurse)
        titleLabel.accessibilityTraits = .header
        addSubview(titleLabel)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var intrinsicContentSize: CGSize {
        titleLabel.intrinsicContentSize
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        titleLabel.frame = bounds
    }

    func apply(progress: CGFloat) {
        titleLabel.alpha = PurseMotion.Collapse.titleOpacity(progress)
    }
}
