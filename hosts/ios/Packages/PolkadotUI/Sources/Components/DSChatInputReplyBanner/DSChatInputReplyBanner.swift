import DesignSystem
import UIKit
internal import SnapKit
internal import UIKit_iOS

final class DSChatInputReplyBanner: UIView {
    private enum Metrics {
        static let shownHeight: CGFloat = 56
        static let panelInset: CGFloat = DSSpacings.small
        static let panelRadius: CGFloat = DSRadii.extraMedium
        static let borderWidth: CGFloat = 4
        static let contentLeading: CGFloat = DSSpacings.extraMedium // after the border
        static let contentVertical: CGFloat = DSSpacings.small
        static let closeSize: CGFloat = 24
        static let closeInset: CGFloat = DSSpacings.extraTiny
    }

    private let panel = UIView()
    private let accentBorder = UIView()
    private let titleLabel = Label()
    private let messageLabel = Label()
    private let closeButton: DSIconButton = .chatInputReplyClose

    private var heightConstraint: Constraint?

    private let heightAnimator: BlockViewAnimatorProtocol = BlockViewAnimator(duration: 0.25)

    var onClose: (() -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(title: String, messageText: String) {
        titleLabel.text = title
        messageLabel.text = messageText
    }

    func show(animated: Bool) {
        setHeight(Metrics.shownHeight, animated: animated)
    }

    func hide(animated: Bool) {
        setHeight(0, animated: animated)
    }

    private func setHeight(_ value: CGFloat, animated: Bool) {
        guard animated else {
            heightConstraint?.update(offset: value)
            return
        }
        heightAnimator.animate(block: {
            self.heightConstraint?.update(offset: value)
            self.superview?.layoutIfNeeded()
        }, completionBlock: nil)
    }
}

private extension DSChatInputReplyBanner {
    func setupViews() {
        clipsToBounds = true

        panel.backgroundColor = .bgSurfaceNested
        panel.clipsToBounds = true
        panel.layer.cornerRadius = Metrics.panelRadius
        panel.layer.cornerCurve = .continuous

        accentBorder.backgroundColor = .strokeTertiary

        titleLabel.typography = .titleTiny
        titleLabel.textColor = .fgPrimary
        titleLabel.numberOfLines = 1

        messageLabel.typography = .bodySmallEmphasized
        messageLabel.textColor = .fgPrimary
        messageLabel.numberOfLines = 1
        messageLabel.lineBreakMode = .byTruncatingTail

        closeButton.onTap = { [weak self] in self?.onClose?() }

        addSubview(panel)
        snp.makeConstraints {
            heightConstraint = $0.height.equalTo(0).constraint
        }
        panel.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview().inset(Metrics.panelInset)
            $0.top.equalToSuperview().offset(Metrics.panelInset)
            $0.bottom.equalToSuperview()
        }

        // Full-height squared left border (clipped by the panel's rounded corners).
        panel.addSubview(accentBorder)
        accentBorder.snp.makeConstraints {
            $0.leading.top.bottom.equalToSuperview()
            $0.width.equalTo(Metrics.borderWidth)
        }

        panel.addSubview(closeButton)
        closeButton.snp.makeConstraints {
            $0.size.equalTo(Metrics.closeSize)
            $0.trailing.equalToSuperview().inset(Metrics.closeInset)
            $0.top.equalToSuperview().offset(Metrics.closeInset)
        }

        let stack = UIStackView(arrangedSubviews: [titleLabel, messageLabel])
        stack.axis = .vertical
        stack.alignment = .leading
        stack.spacing = DSSpacings.zero

        panel.addSubview(stack)
        stack.snp.makeConstraints {
            $0.leading.equalTo(accentBorder.snp.trailing).offset(Metrics.contentLeading)
            $0.trailing.equalTo(closeButton.snp.leading).offset(-DSSpacings.small)
            $0.top.bottom.equalToSuperview().inset(Metrics.contentVertical)
        }
    }
}
