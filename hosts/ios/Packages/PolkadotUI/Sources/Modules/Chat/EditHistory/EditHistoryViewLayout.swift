import UIKit
import SwiftUI
import DesignSystem
internal import UIKit_iOS
internal import SnapKit

public final class EditHistoryViewLayout: UIView {
    public let scrollView: UIScrollView = .create { scrollView in
        scrollView.showsVerticalScrollIndicator = true
        scrollView.alwaysBounceVertical = true
        scrollView.bounces = true
    }

    private let contentStackView: UIStackView = .create { stackView in
        stackView.axis = .vertical
        stackView.spacing = 16
        stackView.alignment = .fill
    }

    private let currentMessageContainerView: RoundedView = .create { view in
        view.cornerRadius = 24
        view.fillColor = .bgSurfaceContainer
        view.shadowOpacity = 0.0
    }

    private let currentMessageStackView: UIStackView = .create { stackView in
        stackView.axis = .vertical
        stackView.spacing = 8
        stackView.alignment = .trailing
    }

    private let titleLabel: Label = .create { (view: Label) in
        view.typography = .titleLarge
        view.textColor = .fgPrimary
        view.textAlignment = .left
        view.text = String(localized: .chatEditHistory)
    }

    private let historyContainerView: RoundedView = .create { view in
        view.cornerRadius = 24
        view.fillColor = .bgSurfaceContainer
        view.shadowOpacity = 0.0
    }

    private let historyContentStackView: UIStackView = .create { stackView in
        stackView.axis = .vertical
        stackView.spacing = 8
        stackView.alignment = .trailing
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupLayout() {
        backgroundColor = .bgSurfaceMain

        addSubview(scrollView)
        scrollView.addSubview(contentStackView)

        contentStackView.addArrangedSubview(currentMessageContainerView)
        contentStackView.addArrangedSubview(titleLabel)
        contentStackView.addArrangedSubview(historyContainerView)

        currentMessageContainerView.addSubview(currentMessageStackView)

        historyContainerView.addSubview(historyContentStackView)

        contentStackView.setCustomSpacing(8, after: titleLabel)

        scrollView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        contentStackView.snp.makeConstraints { make in
            make.top.equalToSuperview().inset(32)
            make.leading.trailing.equalToSuperview().inset(16)
            make.bottom.equalToSuperview().inset(16)
            make.width.equalToSuperview().offset(-32)
        }

        currentMessageStackView.snp.makeConstraints { make in
            make.edges.equalToSuperview().inset(16)
        }

        titleLabel.snp.makeConstraints { make in
            make.leading.equalToSuperview().offset(8)
        }

        historyContentStackView.snp.makeConstraints { make in
            make.edges.equalToSuperview().inset(16)
        }
    }

    public func bind(viewModel: ViewModel) {
        let isOutgoing = viewModel.isOutgoing

        currentMessageStackView.arrangedSubviews.forEach { $0.removeFromSuperview() }
        historyContentStackView.arrangedSubviews.forEach { $0.removeFromSuperview() }

        currentMessageStackView.alignment = isOutgoing ? .trailing : .leading
        historyContentStackView.alignment = isOutgoing ? .trailing : .leading

        let currentStatusColor: UIColor = isOutgoing
            ? UIColor.fgSecondaryInverted
            : UIColor.fgTertiary
        let currentStatusConfig = ChatMessageStatusViewConfiguration(
            timestampText: viewModel.currentMessage.formattedTimestamp,
            textColor: currentStatusColor,
            image: viewModel.currentMessage.statusImage,
            isEdited: false
        )
        let currentConfig: ChatMessageContainerConfiguration = isOutgoing
            ? .editHistoryOutbox(text: viewModel.currentMessage.text, statusConfiguration: currentStatusConfig)
            : .editHistoryInbox(text: viewModel.currentMessage.text, statusConfiguration: currentStatusConfig)
        let currentBubble = currentConfig.makeContentView()
        currentMessageStackView.addArrangedSubview(currentBubble)

        for item in viewModel.historyItems {
            let statusColor: UIColor = isOutgoing
                ? UIColor.fgSecondaryInverted
                : UIColor.fgTertiary
            let statusConfig = ChatMessageStatusViewConfiguration(
                timestampText: item.formattedTimestamp,
                textColor: statusColor,
                image: viewModel.currentMessage.statusImage,
                isEdited: false
            )
            let attributedText = Self.createAttributedText(from: item.diffParts, isOutgoing: isOutgoing)
            let config: ChatMessageContainerConfiguration = isOutgoing
                ? .editHistoryOutboxDiff(attributedText: attributedText, statusConfiguration: statusConfig)
                : .editHistoryInboxDiff(attributedText: attributedText, statusConfiguration: statusConfig)
            let diffBubble = config.makeContentView()
            historyContentStackView.addArrangedSubview(diffBubble)
        }
    }
}

// MARK: - Private functions

extension EditHistoryViewLayout {
    private static func createAttributedText(
        from diffParts: [TextDiffCalculator.DiffPart],
        isOutgoing: Bool
    ) -> AttributedString {
        var result = AttributedString()
        let normalColor = isOutgoing
            ? Color.fgPrimaryInverted
            : Color.fgPrimary
        let addedBgColor = isOutgoing
            ? Color.bgActionTertiaryInverted
            : Color.bgActionTertiary

        for part in diffParts {
            var partString: AttributedString
            switch part {
            case let .unchanged(text):
                partString = AttributedString(text)
                partString.foregroundColor = normalColor

            case let .added(text):
                partString = AttributedString(text)
                partString.foregroundColor = normalColor
                partString.backgroundColor = addedBgColor

            case let .deleted(text):
                partString = AttributedString(text)
                partString.foregroundColor = normalColor
                partString.strikethroughStyle = .single
            }
            result.append(partString)
        }
        return result
    }
}

// MARK: - ViewModel

public extension EditHistoryViewLayout {
    struct ViewModel {
        public typealias DiffPart = TextDiffCalculator.DiffPart

        public struct HistoryItem {
            public let diffParts: [DiffPart]
            public let formattedTimestamp: String

            public init(diffParts: [DiffPart], formattedTimestamp: String) {
                self.diffParts = diffParts
                self.formattedTimestamp = formattedTimestamp
            }
        }

        public struct CurrentMessage {
            public let text: String
            public let formattedTimestamp: String
            public let statusImage: UIImage?

            public init(text: String, formattedTimestamp: String, statusImage: UIImage?) {
                self.text = text
                self.formattedTimestamp = formattedTimestamp
                self.statusImage = statusImage
            }
        }

        public let currentMessage: CurrentMessage
        public let historyItems: [HistoryItem]
        public let isOutgoing: Bool

        public init(currentMessage: CurrentMessage, historyItems: [HistoryItem], isOutgoing: Bool) {
            self.currentMessage = currentMessage
            self.historyItems = historyItems
            self.isOutgoing = isOutgoing
        }
    }
}

// MARK: - ChatMessageContainerConfiguration

private extension ChatMessageContainerConfiguration {
    static func editHistoryOutbox(
        text: String,
        statusConfiguration: ChatMessageStatusViewConfiguration?
    ) -> Self {
        let viewModel = ChatMessageTextView.ViewModel(
            text: text,
            textColor: .fgPrimaryInverted,
            statusPlaceholderImage: statusConfiguration?.placeholderImage
        )
        let view = ChatMessageTextView(viewModel: viewModel)
        let configuration = SwiftUIContentConfiguration(view: view)
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .trailing,
            bubbleColor: .bgSurfaceContainerInverted,
            statusConfiguration: statusConfiguration,
            canReply: false,
            layoutType: .plain,
            identifier: ChatMessageTextView.reuseIdentifier
        )
    }

    static func editHistoryOutboxDiff(
        attributedText: AttributedString,
        statusConfiguration: ChatMessageStatusViewConfiguration?
    ) -> Self {
        let viewModel = ChatMessageTextView.ViewModel(
            attributedText: attributedText,
            textColor: .fgPrimaryInverted,
            statusPlaceholderImage: statusConfiguration?.placeholderImage
        )
        let view = ChatMessageTextView(viewModel: viewModel)
        let configuration = SwiftUIContentConfiguration(view: view)
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .trailing,
            bubbleColor: .bgSurfaceContainerInverted,
            statusConfiguration: statusConfiguration,
            canReply: false,
            layoutType: .plain,
            identifier: ChatMessageTextView.reuseIdentifier
        )
    }

    static func editHistoryInbox(
        text: String,
        statusConfiguration: ChatMessageStatusViewConfiguration?
    ) -> Self {
        let viewModel = ChatMessageTextView.ViewModel(
            text: text,
            textColor: .fgPrimary,
            statusPlaceholderImage: statusConfiguration?.placeholderImage
        )
        let view = ChatMessageTextView(viewModel: viewModel)
        let configuration = SwiftUIContentConfiguration(view: view)
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .leading,
            bubbleColor: .bgSurfaceContainer,
            statusConfiguration: statusConfiguration,
            canReply: false,
            layoutType: .plain,
            identifier: ChatMessageTextView.reuseIdentifier
        )
    }

    static func editHistoryInboxDiff(
        attributedText: AttributedString,
        statusConfiguration: ChatMessageStatusViewConfiguration?
    ) -> Self {
        let viewModel = ChatMessageTextView.ViewModel(
            attributedText: attributedText,
            textColor: .fgPrimary,
            statusPlaceholderImage: statusConfiguration?.placeholderImage
        )
        let view = ChatMessageTextView(viewModel: viewModel)
        let configuration = SwiftUIContentConfiguration(view: view)
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .leading,
            bubbleColor: .bgSurfaceContainer,
            statusConfiguration: statusConfiguration,
            canReply: false,
            layoutType: .plain,
            identifier: ChatMessageTextView.reuseIdentifier
        )
    }
}

// MARK: - Preview

#if DEBUG
    #Preview {
        let layout = EditHistoryViewLayout()

        let viewModel = EditHistoryViewLayout.ViewModel(
            currentMessage: .init(
                text: "Hello, this is the current message!",
                formattedTimestamp: "10:30",
                statusImage: UIImage(resource: .messageDelivered)
            ),
            historyItems: [
                .init(
                    diffParts: [
                        .unchanged("Hello, this is "),
                        .deleted("the old"),
                        .added("the current"),
                        .unchanged(" message!")
                    ],
                    formattedTimestamp: "10:25"
                ),
                .init(
                    diffParts: [
                        .unchanged("Hello, "),
                        .deleted("world"),
                        .added("this is the old"),
                        .unchanged(" message!")
                    ],
                    formattedTimestamp: "10:20"
                )
            ],
            isOutgoing: true
        )

        layout.bind(viewModel: viewModel)
        return layout
    }
#endif
