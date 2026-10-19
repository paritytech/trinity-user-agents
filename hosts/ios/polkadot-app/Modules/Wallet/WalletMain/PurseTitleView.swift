import PolkadotUI
import UIKit

/// The open Purse's header: "Purse", which fades as the card shrinks into the
/// header, and the card's thumbnail, which takes its place once it has.
final class PurseTitleView: UIView {
    private let titleLabel: PolkadotUI.Label = .create { view in
        view.typography = .titleLarge
        view.textColor = .fgPrimary
        view.textAlignment = .center
    }

    private let thumbnail: UIImageView = {
        let view = UIImageView(image: .cashCardWallet)
        view.contentMode = .scaleAspectFill
        view.clipsToBounds = true
        view.layer.cornerRadius = CashCardFace.cornerRadius * PurseMotion.Collapse.dockedWidth
            / PurseMotion.Collapse.fullWidth
        view.isAccessibilityElement = false
        return view
    }()

    override init(frame: CGRect) {
        super.init(frame: frame)
        titleLabel.text = String(localized: .walletMainPurse)
        titleLabel.accessibilityTraits = .header
        addSubview(thumbnail)
        addSubview(titleLabel)
        apply(progress: 0)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var intrinsicContentSize: CGSize {
        let width = max(titleLabel.intrinsicContentSize.width, PurseMotion.Collapse.dockedWidth)
        return CGSize(width: width, height: thumbnailHeight)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        titleLabel.frame = bounds
        thumbnail.frame = CGRect(
            x: (bounds.width - PurseMotion.Collapse.dockedWidth) / 2,
            y: (bounds.height - thumbnailHeight) / 2,
            width: PurseMotion.Collapse.dockedWidth,
            height: thumbnailHeight
        )
    }

    func apply(progress: CGFloat) {
        titleLabel.alpha = PurseMotion.Collapse.titleOpacity(progress)
        thumbnail.alpha = PurseMotion.Collapse.thumbnailOpacity(progress)
    }

    private var thumbnailHeight: CGFloat {
        PurseMotion.Collapse.dockedWidth / CashCardFace.aspectRatio
    }
}
