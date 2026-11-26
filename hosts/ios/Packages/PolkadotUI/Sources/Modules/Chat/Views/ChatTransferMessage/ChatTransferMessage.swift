import UIKit
import DesignSystem
import ExternalAccessibility
internal import SnapKit

public struct ChatTransferMessageConfiguration: HashableContentConfiguration {
    let title: String
    let amountText: String
    let tokenSymbol: String
    let originalAmountText: String?
    let state: ChatTransferMessageConfiguration.DirectionalState
    let statusConfiguration: ChatMessageStatusViewConfiguration
    let backgroundColor: UIColor
    let titleColor: UIColor
    let amountBackgroundColor: UIColor
    let amountTextColor: UIColor
    let tokenSymbolColor: UIColor
    let originalAmountTextColor: UIColor
    let side: ChatBubbleTailSide
    var peerName: String?
    var reclaimAction: ReclaimAction?

    public func makeContentView() -> any UIView & UIContentView {
        ChatTransferMessageView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }
}

public extension ChatTransferMessageConfiguration {
    enum DirectionalState: Hashable {
        case incoming(IncomingState)
        case outgoing(OutgoingState)
    }

    enum IncomingState: Hashable {
        case detecting
        case claiming
        case claimed
        case failed
        case returnedToSender
    }

    enum OutgoingState: Hashable {
        case sending
        case sent
        case claimed
        case failed
        case unclaimed
        case reclaiming
        case reclaimed(partialAmount: String?)
        case reclaimFailed
    }

    /// Equality ignores the handler: the action is identified by its message, not by the closure it carries.
    struct ReclaimAction: Hashable {
        public let onReclaim: () -> Void

        public init(onReclaim: @escaping () -> Void) {
            self.onReclaim = onReclaim
        }

        public static func == (_: Self, _: Self) -> Bool { true }

        public func hash(into _: inout Hasher) {}
    }
}

final class ChatTransferMessageView: UIView, UIContentView, ReactableContentView {
    private let bubbleView = ChatBubbleView()

    private let titleLabel: Label = create {
        $0.lineBreakMode = .byTruncatingMiddle
        $0.typography = .bodyMedium
        $0.numberOfLines = 1
        $0.textAlignment = .left
    }

    /// The original amount sits above the currency + amount + asset row.
    private let amountContainerView: GenericBackgroundView<GenericPairValueView<Label, ChatTransferAmountView>> =
        create { container in
            container.insets = UIEdgeInsets(
                top: DSSpacings.mediumIncreased,
                left: DSSpacings.mediumIncreased,
                bottom: DSSpacings.mediumIncreased,
                right: DSSpacings.mediumIncreased
            )

            let amounts = container.wrappedView
            amounts.makeVertical()
            amounts.spacing = 0
            amounts.stackView.alignment = .fill

            let originalAmount = amounts.fView
            originalAmount.typography = .bodyMedium
            originalAmount.numberOfLines = 1
            originalAmount.textAlignment = .left
            originalAmount.isHidden = true
        }

    var amountView: ChatTransferAmountView {
        amountContainerView.wrappedView.sView
    }

    var originalAmountLabel: Label {
        amountContainerView.wrappedView.fView
    }

    let subtitleIconView: UIImageView = create {
        $0.contentMode = .scaleAspectFit
    }

    let subtitleLabel: Label = create {
        $0.typography = .bodyMedium
        $0.numberOfLines = 3
        $0.textAlignment = .left
        $0.setContentCompressionResistancePriority(.fittingSizeLevel, for: .horizontal)
    }

    private let subtitleStackView: UIStackView = create {
        $0.axis = .horizontal
        $0.alignment = .center
        $0.spacing = 4
    }

    private let reclaimView = ChatTransferReclaimView()

    private let detailsStackView: UIStackView = create {
        $0.axis = .vertical
        $0.alignment = .fill
        $0.spacing = Constants.rowSpacing
    }

    private lazy var statusView = appliedConfiguration.statusConfiguration.makeContentView()

    private var bubbleLeadingConstraint: Constraint?
    private var bubbleTrailingConstraint: Constraint?

    private var appliedConfiguration: ChatTransferMessageConfiguration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    var leadingReactionsAlignmentView: UIView {
        titleLabel
    }

    init(configuration: ChatTransferMessageConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        addSubview(bubbleView)
        subtitleStackView.addArrangedSubview(subtitleIconView)
        subtitleStackView.addArrangedSubview(subtitleLabel)
        detailsStackView.addArrangedSubview(subtitleStackView)
        detailsStackView.addArrangedSubview(reclaimView)

        bubbleView.addSubview(titleLabel)
        bubbleView.addSubview(amountContainerView)
        bubbleView.addSubview(detailsStackView)
        bubbleView.addSubview(statusView)

        bubbleView.snp.makeConstraints {
            $0.width.equalTo(Constants.bubbleWidth).priority(.high)
            $0.width.lessThanOrEqualToSuperview()
            $0.top.equalToSuperview()
            bubbleLeadingConstraint = $0.leading.equalToSuperview().constraint
            bubbleTrailingConstraint = $0.trailing.equalToSuperview().constraint
            $0.bottom.equalToSuperview().priority(.medium)
        }

        titleLabel.snp.makeConstraints {
            $0.leading.equalToSuperview().offset(Constants.bubbleLeadingInset)
            $0.trailing.lessThanOrEqualToSuperview().inset(Constants.bubbleTrailingInset)
            $0.top.equalToSuperview().offset(DSSpacings.extraMedium)
        }

        amountContainerView.snp.makeConstraints {
            $0.width.greaterThanOrEqualTo(150).priority(.medium)
            $0.leading.equalToSuperview().offset(Constants.bubbleLeadingInset)
            $0.trailing.equalToSuperview().inset(Constants.bubbleTrailingInset)
            $0.top.equalTo(titleLabel.snp.bottom).offset(Constants.rowSpacing)
        }

        subtitleIconView.snp.makeConstraints {
            $0.size.equalTo(Constants.statusIconSize)
        }

        detailsStackView.snp.makeConstraints {
            $0.leading.equalToSuperview().offset(Constants.bubbleLeadingInset)
            $0.trailing.equalToSuperview().inset(Constants.bubbleTrailingInset)
            $0.top.equalTo(amountContainerView.snp.bottom).offset(Constants.rowSpacing)
        }

        statusView.snp.makeConstraints {
            $0.leading.greaterThanOrEqualToSuperview().offset(Constants.bubbleLeadingInset)
            $0.trailing.equalToSuperview().inset(Constants.bubbleTrailingInset)
            $0.bottom.equalToSuperview().inset(DSSpacings.small)
            $0.top.equalTo(detailsStackView.snp.bottom).offset(Constants.rowSpacing)
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? ChatTransferMessageConfiguration else { return }
        appliedConfiguration = configuration

        titleLabel.text = configuration.title
        amountView.bind(
            amount: configuration.amountText,
            unit: configuration.tokenSymbol,
            isStruckThrough: configuration.state.isStruckThrough
        )
        amountView.amountLabel.textColor = configuration.amountTextColor
        amountView.unitLabel.textColor = configuration.tokenSymbolColor

        originalAmountLabel.textColor = configuration.originalAmountTextColor
        if let originalAmount = configuration.originalAmountText {
            var attributes = LabelStyle(typography: .bodyMedium).attributes(for: .left)
            attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            originalAmountLabel.attributedText = NSAttributedString(
                string: originalAmount,
                attributes: attributes
            )
            originalAmountLabel.isHidden = false
            subtitleLabel.text = partialClaimText(configuration, originalAmount: originalAmount)
            subtitleLabel.textColor = .fgWarning
            subtitleIconView.isHidden = true
        } else {
            originalAmountLabel.attributedText = nil
            originalAmountLabel.isHidden = true
            subtitleLabel.text = statusText(configuration)
            subtitleLabel.textColor = configuration.state.color
            subtitleIconView.isHidden = configuration.state.icon == nil
            subtitleIconView.image = configuration.state.icon
            subtitleIconView.tintColor = configuration.state.color
        }

        statusView.configuration = configuration.statusConfiguration

        amountContainerView.applyBackgroundStyle(configuration.amountBackgroundColor, cornerRadius: 12)
        titleLabel.textColor = configuration.titleColor

        bubbleView.fillColor = configuration.backgroundColor
        bubbleView.corners = ChatMessageContainerConfiguration.LayoutType.plain
            .cornerRadii(for: configuration.side)

        switch configuration.side {
        case .leading:
            bubbleLeadingConstraint?.isActive = true
            bubbleTrailingConstraint?.isActive = false
        case .trailing:
            bubbleLeadingConstraint?.isActive = false
            bubbleTrailingConstraint?.isActive = true
        }

        applyReclaim(configuration)
    }
}

private extension ChatTransferMessageConfiguration.DirectionalState {
    var icon: UIImage? {
        switch self {
        case .incoming(.detecting),
             .incoming(.claiming):
            UIImage(resource: .iconTransferIn)
        case .outgoing(.sending),
             .outgoing(.sent):
            UIImage(resource: .iconTransferOut)
        case .outgoing(.reclaiming):
            UIImage(resource: .iconRefreshing).withHorizontallyFlippedOrientation()
        case .incoming(.claimed),
             .outgoing(.claimed):
            UIImage(resource: .iconTransferDone)
        case .incoming(.failed),
             .outgoing(.failed),
             .outgoing(.unclaimed),
             .outgoing(.reclaimed),
             .outgoing(.reclaimFailed),
             .incoming(.returnedToSender):
            nil
        }
    }

    var isStruckThrough: Bool {
        switch self {
        case .incoming(.returnedToSender),
             .outgoing(.reclaimed):
            true
        default:
            false
        }
    }

    var bubbleAlpha: CGFloat {
        if case .outgoing(.reclaimed) = self { 0.55 } else { 1 }
    }

    var reclaimTitle: String {
        if case .outgoing(.reclaimFailed) = self {
            String(localized: .transferReclaimRetryAction)
        } else {
            String(localized: .transferReclaimAction)
        }
    }

    var color: UIColor {
        switch self {
        case .incoming(.failed),
             .outgoing(.failed),
             .outgoing(.reclaimFailed),
             .outgoing(.unclaimed),
             .incoming(.returnedToSender):
            .fgWarning
        case .incoming:
            .fgSecondary
        case .outgoing:
            .fgSecondaryInverted
        }
    }

    var title: String {
        switch self {
        case let .incoming(state):
            state.title
        case let .outgoing(state):
            state.title
        }
    }
}

private extension ChatTransferMessageConfiguration.IncomingState {
    var title: String {
        switch self {
        case .detecting:
            String(localized: .transferStatusDetecting)
        case .claiming:
            String(localized: .transferStatusClaiming)
        case .claimed:
            String(localized: .transferStatusFinished)
        case .failed:
            String(localized: .transferStatusError)
        case .returnedToSender:
            String(localized: .transferStatusReturnedToSender)
        }
    }
}

private extension ChatTransferMessageConfiguration.OutgoingState {
    var title: String {
        switch self {
        case .sending:
            String(localized: .transferStatusSending)
        case .sent:
            String(localized: .transferStatusSent)
        case .claimed:
            String(localized: .transferStatusFinished)
        case .failed:
            String(localized: .transferStatusNotSent)
        case .unclaimed:
            String(localized: .transferStatusUnclaimed)
        case .reclaiming:
            String(localized: .transferStatusReclaiming)
        case let .reclaimed(partialAmount):
            if let partialAmount {
                String(localized: .transferStatusReclaimedPartially(amount: partialAmount))
            } else {
                String(localized: .transferStatusReclaimed)
            }
        case .reclaimFailed:
            String(localized: .transferStatusReclaimFailed)
        }
    }
}

// MARK: - AccessibilityBound

extension ChatTransferMessageView: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [
            .init(bubbleView, AccessibilityID.Chat.transferMessageBubble),
            .init(subtitleLabel, AccessibilityID.Chat.transferStatusLabel)
        ]
    }
}

private extension ChatTransferMessageView {
    func statusText(_ configuration: ChatTransferMessageConfiguration) -> String {
        if case .outgoing(.claimed) = configuration.state, let peerName = configuration.peerName {
            return String(localized: .transferStatusReceivedBy(username: peerName))
        }
        return configuration.state.title
    }

    func partialClaimText(_ configuration: ChatTransferMessageConfiguration, originalAmount: String) -> String {
        switch configuration.state {
        case .incoming:
            String(localized: .transferStatusAmountReceived(
                received: configuration.amountText,
                sent: originalAmount
            ))
        case .outgoing:
            if let peerName = configuration.peerName {
                String(localized: .transferStatusAmountReceivedBy(
                    username: peerName,
                    received: configuration.amountText,
                    sent: originalAmount
                ))
            } else {
                String(localized: .transferStatusAmountDiffers)
            }
        }
    }
}

private extension ChatTransferMessageView {
    enum Constants {
        static let bubbleWidth: CGFloat = 250
        static let bubbleLeadingInset = DSSpacings.medium
        static let bubbleTrailingInset = DSSpacings.small
        static let rowSpacing = DSSpacings.small
        static let statusIconSize: CGFloat = 14
        static let spinAnimationKey = "reclaimSpin"
    }

    func applyReclaim(_ configuration: ChatTransferMessageConfiguration) {
        reclaimView.bind(title: configuration.state.reclaimTitle, action: configuration.reclaimAction)

        bubbleView.alpha = configuration.state.bubbleAlpha
        let isReclaiming = configuration.state == .outgoing(.reclaiming)
        updateIconSpin(isSpinning: isReclaiming)
        amountView.setShimmering(isReclaiming)
    }

    func updateIconSpin(isSpinning: Bool) {
        let layer = subtitleIconView.layer
        guard isSpinning else {
            layer.removeAnimation(forKey: Constants.spinAnimationKey)
            return
        }
        guard layer.animation(forKey: Constants.spinAnimationKey) == nil else { return }

        let spin = CABasicAnimation(keyPath: "transform.rotation.z")
        spin.fromValue = 0
        spin.toValue = 2 * Double.pi
        spin.duration = 1
        spin.repeatCount = .infinity
        layer.add(spin, forKey: Constants.spinAnimationKey)
    }
}
