import DesignSystem
import UIKit

internal import SnapKit

final class ScrollToBottomButton: UIControl {
    var unreadCount: Int = 0 { didSet { updateBadge() } }
    var maxBadgeValue: Int = 99 { didSet { updateBadge() } }

    private let iconButton: DSIconButton = .chatScrollToBottom

    private let badgeLabel = InsettableLabel()

    override init(frame: CGRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    override var intrinsicContentSize: CGSize {
        iconButton.intrinsicContentSize
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        badgeLabel.layer.cornerRadius = badgeLabel.bounds.height / 2
    }
}

// MARK: - Private functions

extension ScrollToBottomButton {
    private func setup() {
        clipsToBounds = false

        iconButton.onTap = { [weak self] in
            self?.sendActions(for: .touchUpInside)
        }
        addSubview(iconButton)
        iconButton.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        badgeLabel.isUserInteractionEnabled = false
        badgeLabel.insets = UIEdgeInsets(top: 2, left: 4, bottom: 2, right: 4)
        badgeLabel.typography = .paragraphSmall
        badgeLabel.textColor = .fgPrimaryInverted
        badgeLabel.backgroundColor = .bgSurfaceContainerInverted
        badgeLabel.textAlignment = .center
        badgeLabel.clipsToBounds = true
        badgeLabel.setHidden(true)
        badgeLabel.layer.borderWidth = 0.5

        // CGColor borders don't re-resolve on theme change; re-apply manually.
        registerForTraitChanges([DSThemeTrait.self]) { (button: ScrollToBottomButton, _) in
            button.applyBadgeBorderColor()
        }
        applyBadgeBorderColor()

        addSubview(badgeLabel)
        badgeLabel.snp.makeConstraints { make in
            make.height.greaterThanOrEqualTo(16)
            make.width.greaterThanOrEqualTo(badgeLabel.snp.height)
            make.centerX.equalTo(snp.trailing).offset(-1)
            make.centerY.equalTo(snp.top).offset(1)
        }

        updateBadge()
    }

    private func applyBadgeBorderColor() {
        badgeLabel.layer.borderColor = UIColor.strokePrimary.resolvedColor(with: traitCollection).cgColor
    }

    private func updateBadge() {
        let value = max(0, unreadCount)
        guard value > 0 else {
            badgeLabel.setHidden(true)
            setNeedsLayout()
            return
        }

        badgeLabel.text = (value > maxBadgeValue) ? "\(maxBadgeValue)+" : "\(value)"
        badgeLabel.setHidden(false)
        setNeedsLayout()
    }
}
