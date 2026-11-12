import UIKit
import SwiftUI
import DesignSystem
internal import SnapKit

// The wrapper that defines the Bubble and Metadata
public struct ChatMessageContainerConfiguration: HashableContentConfiguration {
    public enum InnerContentLayout: Hashable {
        case leading
        case fill
        case sideAligned
    }

    public enum StatusLayout {
        // attached to bubble trailing side
        case bubble
        // attached to content trailing side
        case content
    }

    public enum LayoutType: Hashable {
        case plain
        case groupedTop
        case groupedMiddle
        case groupedBottom

        public func cornerRadii(for side: ChatBubbleTailSide) -> CornersConfiguration {
            let large: CGFloat = DSRadii.mediumIncreased
            let small: CGFloat = DSRadii.small
            switch (self, side) {
            case (.plain, _):
                return .all(large)
            case (.groupedTop, .leading):
                return .all(large).corners(.bottomLeft, small)
            case (.groupedTop, .trailing):
                return .all(large).corners(.bottomRight, small)
            case (.groupedMiddle, .leading):
                return .all(large).corners([.topLeft, .bottomLeft], small)
            case (.groupedMiddle, .trailing):
                return .all(large).corners([.topRight, .bottomRight], small)
            case (.groupedBottom, .leading):
                return .all(large).corners(.topLeft, small)
            case (.groupedBottom, .trailing):
                return .all(large).corners(.topRight, small)
            }
        }
    }

    let innerContentConfiguration: any HashableContentConfiguration
    let innerContentLayout: InnerContentLayout
    let identifier: String

    // Bubble Props
    let side: ChatBubbleTailSide
    let bubbleColor: UIColor
    let bubbleStrokeColor: UIColor?
    let bubbleStrokeWidth: CGFloat

    // Metadata
    let statusConfiguration: ChatMessageStatusViewConfiguration?
    let replyInfo: ReplyInfo?
    let canReply: Bool
    let addReaction: AddReactionViewModel?
    let messageReaction: MessageReactionViewModel?

    let contentInsets: UIEdgeInsets
    /// Additional insets applied to the status view on top of `contentInsets`.
    /// Only `bottom` and `right` are used because the status is pinned to the bottom-right corner.
    /// Respects StatusLayout type
    let statusViewInsets: UIEdgeInsets
    let layoutType: LayoutType
    let statusLayout: StatusLayout
    private(set) var menuProvider: (() -> [UIMenuElement])?

    var canReact: Bool {
        addReaction != nil
    }

    public init(
        innerContent: any HashableContentConfiguration,
        innerContentLayout: InnerContentLayout = .fill,
        side: ChatBubbleTailSide,
        bubbleColor: UIColor,
        bubbleStrokeColor: UIColor? = nil,
        bubbleStrokeWidth: CGFloat = 0,
        statusConfiguration: ChatMessageStatusViewConfiguration? = nil,
        statusLayout: StatusLayout = .bubble,
        replyInfo: ReplyInfo? = nil,
        canReply: Bool = true,
        addReaction: AddReactionViewModel? = nil,
        messageReaction: MessageReactionViewModel? = nil,
        contentInsets: UIEdgeInsets = .init(top: 8, left: 14, bottom: 8, right: 8),
        statusViewInsets: UIEdgeInsets = .zero,
        layoutType: LayoutType = .plain,
        identifier: String,
        menuProvider: (() -> [UIMenuElement])? = nil
    ) {
        innerContentConfiguration = innerContent
        self.innerContentLayout = innerContentLayout
        self.side = side
        self.bubbleColor = bubbleColor
        self.bubbleStrokeColor = bubbleStrokeColor
        self.bubbleStrokeWidth = bubbleStrokeWidth
        self.statusConfiguration = statusConfiguration
        self.statusLayout = statusLayout
        self.replyInfo = replyInfo
        self.canReply = canReply
        self.addReaction = addReaction
        self.messageReaction = messageReaction
        self.contentInsets = contentInsets
        self.statusViewInsets = statusViewInsets
        self.layoutType = layoutType
        self.identifier = identifier
        self.menuProvider = menuProvider
    }

    public func makeContentView() -> any UIView & UIContentView {
        ChatMessageContainerView(configuration: self)
    }

    public static func == (
        lhs: ChatMessageContainerConfiguration,
        rhs: ChatMessageContainerConfiguration
    ) -> Bool {
        lhs.side == rhs.side &&
            AnyHashable(lhs.innerContentConfiguration) == AnyHashable(rhs.innerContentConfiguration) &&
            lhs.statusConfiguration == rhs.statusConfiguration &&
            lhs.replyInfo == rhs.replyInfo &&
            lhs.identifier == rhs.identifier &&
            lhs.canReply == rhs.canReply &&
            lhs.messageReaction == rhs.messageReaction &&
            lhs.layoutType == rhs.layoutType
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(side)
        hasher.combine(innerContentConfiguration)
        hasher.combine(replyInfo)
        hasher.combine(canReply)
        hasher.combine(identifier)
        hasher.combine(statusConfiguration)
        hasher.combine(messageReaction)
        hasher.combine(layoutType)
    }

    public mutating func setActions(_ actions: (() -> [UIMenuElement])?) {
        menuProvider = actions
    }
}

// MARK: - View

final class ChatMessageContainerView: UIView, UIContentView {
    typealias Configuration = ChatMessageContainerConfiguration

    let bubbleView = ChatBubbleView()

    // Dynamic Subviews
    private var innerContentView: (UIView & UIContentView)?
    private var replyQuoteView: ChatReplyQuoteView?
    private var statusView: (UIView & UIContentView)?
    private var reactionsView: (UIView & UIContentView)?

    // MARK: - Constraints References

    private var bubbleLeadingConstraint: Constraint?
    private var bubbleTrailingConstraint: Constraint?
    private var bubbleTopConstraint: Constraint?

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    private(set) var appliedConfiguration: Configuration

    var onReply: (() -> Void)?
    var onQuoteTap: ((String) -> Void)?
    private var panGestureRecognizer: UIPanGestureRecognizer?
    private let feedbackGenerator = UIImpactFeedbackGenerator(style: .medium)
    private let swipeReplyThreshold: CGFloat = 60
    private let swipeReplyIconSize: CGFloat = 32
    private let swipeReplyIconHiddenTransform = CGAffineTransform(scaleX: 0.01, y: 0.01)

    private lazy var swipeReplyIconView: UIImageView = {
        let imageView = UIImageView(image: UIImage(resource: .swipeReply))
        imageView.contentMode = .scaleAspectFit
        imageView.alpha = 0
        imageView.transform = swipeReplyIconHiddenTransform
        return imageView
    }()

    init(configuration: Configuration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupStructure()
        apply(configuration)

        setupSwipeToReply()
        setupContextMenu()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) { fatalError() }

    private func setupContextMenu() {
        let interaction = UIContextMenuInteraction(delegate: self)
        bubbleView.addInteraction(interaction)
    }

    // MARK: - Setup

    private func setupStructure() {
        addSubview(swipeReplyIconView)
        swipeReplyIconView.snp.makeConstraints {
            $0.size.equalTo(swipeReplyIconSize)
            $0.centerY.equalToSuperview()
            $0.trailing.equalToSuperview()
        }

        addSubview(bubbleView)
        bubbleView.snp.makeConstraints {
            $0.width.lessThanOrEqualToSuperview().multipliedBy(0.83)
            $0.width.equalTo(
                UIView.layoutFittingCompressedSize.width
            )
            .priority(UILayoutPriority.fittingSizeLevel.rawValue)

            // Store top constraint to apply dynamic spacing
            bubbleTopConstraint = $0.top.equalToSuperview().constraint
            // Store these to toggle side logic later
            bubbleLeadingConstraint = $0.leading.equalToSuperview().constraint
            bubbleTrailingConstraint = $0.trailing.equalToSuperview().constraint

            $0.bottom.equalToSuperview().priority(.high)
        }
    }

    // MARK: - Apply Logic

    private func apply(_ any: UIContentConfiguration) {
        guard let config = any as? Configuration else { return }
        appliedConfiguration = config

        bubbleView.fillColor = config.bubbleColor
        bubbleView.strokeColor = config.bubbleStrokeColor
        bubbleView.strokeWidth = config.bubbleStrokeWidth
        bubbleView.corners = config.layoutType.cornerRadii(for: config.side)

        switch config.side {
        case .leading:
            bubbleLeadingConstraint?.isActive = true
            bubbleTrailingConstraint?.isActive = false
        case .trailing:
            bubbleLeadingConstraint?.isActive = false
            bubbleTrailingConstraint?.isActive = true
        }

        updateSubviews(with: config)
        updateLayoutConstraints()

        panGestureRecognizer?.isEnabled = config.canReply
    }

    private func updateSubviews(with config: Configuration) {
        // A. Reply Quote
        if let replyInfo = config.replyInfo {
            let quoteView = replyQuoteView ?? ChatReplyQuoteView()
            if replyQuoteView == nil {
                replyQuoteView = quoteView
                bubbleView.addSubview(quoteView)
                let tap = UITapGestureRecognizer(target: self, action: #selector(handleQuoteTap))
                quoteView.addGestureRecognizer(tap)
                quoteView.isUserInteractionEnabled = true
            }
            let style: ChatReplyQuoteView.Style = config.side == .leading ? .inbox : .outbox
            quoteView.configure(username: replyInfo.username, preview: replyInfo.preview, style: style)
        } else {
            replyQuoteView?.removeFromSuperview()
            replyQuoteView = nil
        }
        replyQuoteView.map { bubbleView.bringSubviewToFront($0) }

        // B. Inner Content (Smart Reuse)
        let innerContentConfiguration = config.innerContentConfiguration
        innerContentView.apply(innerContentConfiguration) { bubbleView.addSubview($0) }
        innerContentView.map { bubbleView.bringSubviewToFront($0) }

        // C. Status View
        statusView.apply(config.statusConfiguration) { bubbleView.addSubview($0) }
        statusView.map { bubbleView.bringSubviewToFront($0) }

        // D. Reactions
        let reactionsConfiguration: SwiftUIContentConfiguration?
        if let configuration = config.messageReaction,
           !configuration.reactions.isEmpty {
            let reactions = ReactionsDisplayView(
                reactions: configuration.reactions,
                onReactionTap: configuration.onReactionTap,
                onReactionLongPress: configuration.onReactionLongPress
            )
            reactionsConfiguration = SwiftUIContentConfiguration(view: reactions)
        } else {
            reactionsConfiguration = nil
        }

        reactionsView.apply(reactionsConfiguration) { addSubview($0) }
        reactionsView.map { bringSubviewToFront($0) }
    }

    // MARK: - Centralized Layout Logic

    private func updateLayoutConstraints() {
        guard let innerView = innerContentView else { return }

        let insets = appliedConfiguration.contentInsets

        // 1. LAYOUT QUOTE (Header)
        if let quoteView = replyQuoteView {
            quoteView.snp.remakeConstraints { make in
                make.top.equalTo(bubbleView).offset(4)
                make.leading.equalTo(bubbleView).offset(4)
                make.trailing.equalTo(bubbleView).offset(-4)
            }
        }

        // 2. LAYOUT INNER CONTENT (Body)
        innerView.snp.remakeConstraints { make in
            switch appliedConfiguration.innerContentLayout {
            case .leading:
                applyLeadingInnerContentLayout(make: make, leadingInset: insets.left, trailingInset: insets.right)
            case .fill:
                applyFillInnerContentLayout(make: make, leadingInset: insets.left, trailingInset: insets.right)
            case .sideAligned:
                applySideAlignedInnerContentLayout(make: make, leadingInset: insets.left, trailingInset: insets.right)
            }

            // C. Top (Depends on Quote)
            if let quoteView = replyQuoteView {
                // verify other messages
                make.top.equalTo(quoteView.snp.bottom).offset(insets.top)
            } else {
                make.top.equalTo(bubbleView).inset(insets.top)
            }

            // D. Bottom (Always Pin to Bubble Bottom)
            make.bottom.equalTo(bubbleView).inset(insets.bottom)
        }

        // 3. LAYOUT STATUS (Footer)
        if let statView = statusView {
            let statusInsets = appliedConfiguration.statusViewInsets
            statView.snp.remakeConstraints { make in
                let trailingAnchor =
                    switch appliedConfiguration.statusLayout {
                    case .bubble: bubbleView
                    case .content: innerView
                    }
                make.bottom.equalTo(bubbleView).inset(statusInsets.bottom)
                make.trailing.equalTo(trailingAnchor).inset(statusInsets.right)

                // Content Compression: Ensure timestamp doesn't get squashed by long text
                statView.setContentCompressionResistancePriority(.required, for: .horizontal)
                statView.setContentHuggingPriority(.required, for: .horizontal)
            }
        }

        // 4. LAYOUT REACTIONS
        if let reactionsView {
            reactionsView.snp.remakeConstraints {
                if let reactableView = innerView as? ReactableContentView {
                    let item = reactableView.leadingReactionsAlignmentView.snp.leading
                    $0.leading.equalTo(item)
                } else {
                    $0.leading.equalTo(bubbleView).inset(insets.left)
                }

                $0.top.equalTo(bubbleView.snp.bottom).inset(DSSpacings.extraSmall)
                $0.trailing.lessThanOrEqualTo(bubbleView).inset(insets.right)

                $0.bottom.equalToSuperview()
            }
        }
    }

    private func applyLeadingInnerContentLayout(
        make: ConstraintMaker,
        leadingInset: CGFloat,
        trailingInset: CGFloat
    ) {
        make.leading.equalTo(bubbleView).inset(leadingInset)
        make.trailing.lessThanOrEqualTo(bubbleView).inset(trailingInset)
    }

    private func applyFillInnerContentLayout(
        make: ConstraintMaker,
        leadingInset: CGFloat,
        trailingInset: CGFloat
    ) {
        make.leading.equalTo(bubbleView).inset(leadingInset)
        make.trailing.equalTo(bubbleView).inset(trailingInset)
    }

    private func applySideAlignedInnerContentLayout(
        make: ConstraintMaker,
        leadingInset: CGFloat,
        trailingInset: CGFloat
    ) {
        switch appliedConfiguration.side {
        case .leading:
            make.leading.equalTo(bubbleView).inset(leadingInset)
            make.trailing.lessThanOrEqualTo(bubbleView).inset(trailingInset)
        case .trailing:
            make.trailing.equalTo(bubbleView).inset(trailingInset)
            make.leading.greaterThanOrEqualTo(bubbleView).inset(leadingInset)
        }
    }
}

extension ChatMessageContainerView: UIGestureRecognizerDelegate {
    // MARK: - UIGestureRecognizerDelegate

    override func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard gestureRecognizer == panGestureRecognizer,
              let panGesture = gestureRecognizer as? UIPanGestureRecognizer else {
            return true
        }
        let translation = panGesture.translation(in: self)
        // Only allow horizontal swipes from right to left
        return abs(translation.x) > abs(translation.y) && translation.x < 0
    }
}

extension ChatMessageContainerView {
    @objc
    private func handleQuoteTap() {
        guard let messageId = appliedConfiguration.replyInfo?.messageId else { return }
        onQuoteTap?(messageId)
    }

    private func setupSwipeToReply() {
        let panGesture = UIPanGestureRecognizer(target: self, action: #selector(handlePanGesture))
        panGesture.delegate = self
        addGestureRecognizer(panGesture)
        panGestureRecognizer = panGesture
    }

    @objc
    private func handlePanGesture(_ gesture: UIPanGestureRecognizer) {
        let translation = gesture.translation(in: self)
        let velocity = gesture.velocity(in: self)
        let effectiveTranslation = -translation.x

        switch gesture.state {
        case .began:
            feedbackGenerator.prepare()

        case .changed:
            guard effectiveTranslation > 0 else { return }

            let dampingFactor: CGFloat = 0.4
            let translationWithDamping = effectiveTranslation * dampingFactor
            let maxTranslation = swipeReplyThreshold * 1.5
            let clampedTranslation = min(translationWithDamping, maxTranslation)

            bubbleView.transform = CGAffineTransform(translationX: -clampedTranslation, y: 0)

            let progress = min(clampedTranslation / (swipeReplyThreshold * dampingFactor), 1.0)
            let scale = max(0.01, progress)
            swipeReplyIconView.transform = CGAffineTransform(scaleX: scale, y: scale)
            swipeReplyIconView.alpha = progress

        case .ended,
             .cancelled,
             .failed:
            let didCrossThreshold = (effectiveTranslation >= swipeReplyThreshold)

            UIView.animate(
                withDuration: 0.3,
                delay: 0,
                usingSpringWithDamping: 0.7,
                initialSpringVelocity: abs(velocity.x) / 1_000,
                options: [.allowUserInteraction, .beginFromCurrentState]
            ) {
                self.bubbleView.transform = .identity
                self.swipeReplyIconView.alpha = 0
                self.swipeReplyIconView.transform = self.swipeReplyIconHiddenTransform
            }

            if didCrossThreshold {
                feedbackGenerator.impactOccurred(intensity: 1.0)
                onReply?()
            }

        default: break
        }
    }
}

public extension ChatMessageContainerConfiguration {
    struct AddReactionViewModel: Equatable {
        public let quickEmojis: [String]
        public let allSections: [EmojiPickerInline.Section]
        public let onReactionTap: ((String) -> Void)?

        public init(
            quickEmojis: [String],
            allSections: [EmojiPickerInline.Section],
            onReactionTap: ((String) -> Void)?
        ) {
            self.quickEmojis = quickEmojis
            self.allSections = allSections
            self.onReactionTap = onReactionTap
        }

        public static func == (lhs: Self, rhs: Self) -> Bool {
            lhs.quickEmojis == rhs.quickEmojis && lhs.allSections.map(\.title) == rhs.allSections.map(\.title)
        }
    }

    struct MessageReactionViewModel: Hashable {
        public let reactions: [ReactionViewModel]
        public let onReactionTap: ((String) -> Void)?
        public let onReactionLongPress: (() -> Void)?

        public init(
            reactions: [ReactionViewModel],
            onReactionTap: ((String) -> Void)?,
            onReactionLongPress: (() -> Void)?
        ) {
            self.reactions = reactions
            self.onReactionTap = onReactionTap
            self.onReactionLongPress = onReactionLongPress
        }

        public static func == (lhs: Self, rhs: Self) -> Bool {
            lhs.reactions == rhs.reactions
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(reactions)
        }
    }
}

public extension ChatMessageContainerConfiguration {
    struct ReplyInfo: Hashable {
        public let username: String
        public let preview: String
        public let messageId: String

        public init(
            username: String,
            preview: String,
            messageId: String
        ) {
            self.username = username
            self.preview = preview
            self.messageId = messageId
        }
    }
}

#if DEBUG
    #Preview {
        let statusConfig = ChatMessageStatusViewConfiguration.read
        let viewModel = ChatMessageTextView.ViewModel(
            text: "tests asdasdasd asdasdasd asd",
            textColor: .white,
            statusPlaceholderImage: statusConfig.placeholderImage
        )
        let view = ChatMessageTextView(viewModel: viewModel)
        let configuration = SwiftUIContentConfiguration(view: view)
        ChatMessageContainerConfiguration(
            innerContent: configuration,
            innerContentLayout: .leading,
            side: .leading,
            bubbleColor: .black,
            statusConfiguration: statusConfig,
            replyInfo: ChatMessageContainerConfiguration.ReplyInfo(
                username: "user",
                preview: "reply text",
                messageId: "asd"
            ),
            canReply: false,
            identifier: viewModel.rawText
        ).makeContentView()
    }

    #Preview("Failed") {
        ChatMessageContainerConfiguration.outboxRichText(
            text: "Hi! I'd like to add you as a contact",
            statusConfiguration: .failed
        )
        .makeContentView()
    }
#endif
