import UIKit
import DesignSystem

public struct DSTabBarChip {
    public let id: UUID
    public let name: String
    public let icon: ImageViewModelProtocol?

    public init(id: UUID, name: String, icon: ImageViewModelProtocol?) {
        self.id = id
        self.name = name
        self.icon = icon
    }
}

final class DSTabBarChipView: UIView {
    private(set) var chip: DSTabBarChip?

    var isSelectedTab: Bool = false {
        didSet {
            guard isSelectedTab != oldValue else { return }
            nameLabel.textColor = isSelectedTab ? .fgPrimary : .fgSecondary
        }
    }

    private let iconView = UIImageView()
    private let nameLabel = UILabel()

    override init(frame: CGRect) {
        super.init(frame: frame)

        iconView.contentMode = .scaleAspectFill
        iconView.clipsToBounds = true
        iconView.layer.cornerRadius = DSTabBarPanelLayout.iconSize / 2
        addSubview(iconView)

        nameLabel.font = .labelSmallEmphasized
        nameLabel.textAlignment = .center
        nameLabel.textColor = .fgSecondary
        nameLabel.lineBreakMode = .byTruncatingTail
        nameLabel.numberOfLines = DSTabBarPanelLayout.labelMaxLines
        addSubview(nameLabel)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func apply(_ chip: DSTabBarChip) {
        let previous = self.chip
        self.chip = chip

        nameLabel.text = chip.name
        accessibilityLabel = chip.name
        isAccessibilityElement = true
        accessibilityTraits = .button
        setNeedsLayout()

        guard previous?.id != chip.id else {
            return
        }

        previous?.icon?.cancel(on: iconView)
        loadIcon(chip.icon)
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        let icon = DSTabBarPanelLayout.iconSize
        iconView.frame = CGRect(x: ((bounds.width - icon) / 2).rounded(), y: 0, width: icon, height: icon)
        nameLabel.frame = CGRect(
            x: 0,
            y: icon + DSTabBarPanelLayout.iconLabelGap,
            width: bounds.width,
            height: Self.fittingLabelHeight(for: nameLabel.text ?? "", width: bounds.width)
        )
    }

    static func fittingLabelHeight(for text: String, width: CGFloat) -> CGFloat {
        let maxHeight = DSTabBarPanelLayout.labelHeight
        let box = CGSize(width: width, height: maxHeight)
        let measured = (text as NSString).boundingRect(
            with: box,
            options: [.usesLineFragmentOrigin, .usesFontLeading],
            attributes: [.font: UIFont.labelSmallEmphasized],
            context: nil
        ).height

        return min(max(measured.rounded(.up), DSTabBarPanelLayout.labelLineHeight), maxHeight)
    }
}

private extension DSTabBarChipView {
    func loadIcon(_ icon: ImageViewModelProtocol?) {
        iconView.image = nil

        let side = DSTabBarPanelLayout.iconSize
        icon?.loadImage(
            on: iconView,
            targetSize: CGSize(width: side, height: side),
            cornerRadius: side / 2,
            animated: false
        )
    }
}
