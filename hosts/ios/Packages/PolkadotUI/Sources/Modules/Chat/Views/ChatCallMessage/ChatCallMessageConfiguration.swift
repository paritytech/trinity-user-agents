import UIKit
import DesignSystem
internal import SnapKit

public struct ChatCallMessageConfiguration: HashableContentConfiguration {
    public enum CallType: Hashable {
        case audio
        case video
    }

    public enum State: Hashable {
        case calling
        case active
        case finished(duration: String)
        case missed
        case cancelled
    }

    public enum Direction: Hashable {
        case incoming
        case outgoing
    }

    let callType: CallType
    let direction: Direction
    let state: State
    let statusConfiguration: ChatMessageStatusViewConfiguration?
    let onTap: (() -> Void)?

    public init(
        callType: CallType,
        direction: Direction,
        state: State,
        statusConfiguration: ChatMessageStatusViewConfiguration? = nil,
        onTap: (() -> Void)? = nil
    ) {
        self.callType = callType
        self.direction = direction
        self.state = state
        self.statusConfiguration = statusConfiguration
        self.onTap = onTap
    }

    public func makeContentView() -> any UIView & UIContentView {
        ChatCallMessageView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }

    public static func == (
        lhs: ChatCallMessageConfiguration,
        rhs: ChatCallMessageConfiguration
    ) -> Bool {
        lhs.callType == rhs.callType
            && lhs.direction == rhs.direction
            && lhs.state == rhs.state
            && lhs.statusConfiguration == rhs.statusConfiguration
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(callType)
        hasher.combine(direction)
        hasher.combine(state)
        hasher.combine(statusConfiguration)
    }
}

// MARK: - View

final class ChatCallMessageView: UIView, UIContentView {
    private let iconContainerView: UIView = create {
        $0.layer.cornerRadius = 20
        $0.clipsToBounds = true
    }

    private let iconImageView: UIImageView = create {
        $0.contentMode = .scaleAspectFit
    }

    private let titleLabel: Label = create {
        $0.typography = .paragraphLarge
        $0.numberOfLines = 1
        $0.lineBreakMode = .byTruncatingTail
    }

    private let subtitleIcon: UIImageView = create {
        $0.contentMode = .scaleAspectFit
    }

    private let subtitleLabel: Label = create {
        $0.typography = .bodySmallEmphasized
        $0.numberOfLines = 1
    }

    private let textStack: UIStackView = create {
        $0.axis = .vertical
        $0.alignment = .fill
        $0.spacing = 0
    }

    private let detailsTextStack: UIStackView = create {
        $0.spacing = 2
        $0.axis = .horizontal
    }

    private let bottomRowStack: UIStackView = create {
        $0.axis = .horizontal
        $0.alignment = .center
        $0.spacing = DSSpacings.small
    }

    private var statusView: (UIView & UIContentView)?

    private let contentStack: UIStackView = create {
        $0.axis = .horizontal
        $0.alignment = .center
        $0.spacing = DSSpacings.small
    }

    private let outerStack: UIStackView = create {
        $0.axis = .horizontal
        $0.alignment = .bottom
        $0.spacing = 4
        $0.isLayoutMarginsRelativeArrangement = true
    }

    var appliedConfiguration: ChatCallMessageConfiguration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: ChatCallMessageConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        setupTapGesture()
        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - Setup

private extension ChatCallMessageView {
    func setupViews() {
        iconContainerView.addSubview(iconImageView)

        detailsTextStack.addArrangedSubview(subtitleIcon)
        detailsTextStack.addArrangedSubview(subtitleLabel)

        bottomRowStack.addArrangedSubview(detailsTextStack)

        textStack.addArrangedSubview(titleLabel)
        textStack.addArrangedSubview(bottomRowStack)

        contentStack.addArrangedSubview(iconContainerView)
        contentStack.addArrangedSubview(textStack)
        contentStack.isLayoutMarginsRelativeArrangement = true
        contentStack.layoutMargins = UIEdgeInsets(
            top: DSSpacings.small,
            left: DSSpacings.extraMedium,
            bottom: DSSpacings.small,
            right: DSSpacings.small
        )
        addSubview(contentStack)
        contentStack.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        iconContainerView.snp.makeConstraints {
            $0.size.equalTo(40)
        }

        iconImageView.snp.makeConstraints {
            $0.center.equalToSuperview()
            $0.size.equalTo(20)
        }

        subtitleIcon.snp.makeConstraints {
            $0.size.equalTo(16)
        }
    }

    func setupTapGesture() {
        let recognizer = UITapGestureRecognizer(target: self, action: #selector(handleBubbleTap))
        isUserInteractionEnabled = true
        addGestureRecognizer(recognizer)
    }

    @objc func handleBubbleTap() {
        appliedConfiguration.onTap?()
    }
}

// MARK: - Update

private extension ChatCallMessageView {
    func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? ChatCallMessageConfiguration else { return }
        appliedConfiguration = configuration

        titleLabel.text = configuration.title
        titleLabel.textColor = configuration.direction.titleColor

        subtitleIcon.image = configuration.state.subtitleIcon
        subtitleIcon.tintColor = configuration.state.subtitleIconTint

        subtitleLabel.text = configuration.state.subtitle
        subtitleLabel.textColor = configuration.direction.subtitleColor

        statusView.apply(configuration.statusConfiguration) { view in
            view.setContentHuggingPriority(.required, for: .horizontal)
            view.setContentCompressionResistancePriority(.required, for: .horizontal)
            bottomRowStack.addArrangedSubview(view)
        }

        iconContainerView.backgroundColor = configuration.direction.iconContainerColor
        iconImageView.image = configuration.callType.icon.withRenderingMode(.alwaysTemplate)
        iconImageView.tintColor = configuration.iconTintColor

        setNeedsLayout()
    }
}

// MARK: - Previews

#if DEBUG
    import SwiftUI

    private struct PreviewTimestampFormatter: TimestampFormatting {
        private let formatter: DateFormatter = {
            let formatter = DateFormatter()
            formatter.dateFormat = "HH:mm"
            return formatter
        }()

        func string(for date: Date, now _: Date) -> String {
            formatter.string(from: date)
        }
    }

    #Preview {
        let messages: [any HashableContentConfiguration] = [
            ChatCallMessageConfiguration.inbox(
                callType: .audio,
                state: .calling,
                statusConfiguration: .inbox(date: .now, formatter: PreviewTimestampFormatter())
            ),
            ChatCallMessageConfiguration.inbox(
                callType: .video,
                state: .active,
                statusConfiguration: .inbox(date: .now, formatter: PreviewTimestampFormatter())
            ),
            ChatCallMessageConfiguration.inbox(
                callType: .audio,
                state: .finished(duration: "3 min"),
                statusConfiguration: .inbox(date: .now, formatter: PreviewTimestampFormatter())
            ),
            ChatCallMessageConfiguration.inbox(
                callType: .video,
                state: .missed,
                statusConfiguration: .inbox(date: .now, formatter: PreviewTimestampFormatter())
            ),
            ChatCallMessageConfiguration.outbox(
                callType: .audio,
                state: .calling,
                statusConfiguration: .outbox(date: .now, formatter: PreviewTimestampFormatter(), status: .delivered)
            ),
            ChatCallMessageConfiguration.outbox(
                callType: .video,
                state: .active,
                statusConfiguration: .outbox(date: .now, formatter: PreviewTimestampFormatter(), status: .delivered)
            ),
            ChatCallMessageConfiguration.outbox(
                callType: .audio,
                state: .finished(duration: "3 min"),
                statusConfiguration: .outbox(date: .now, formatter: PreviewTimestampFormatter(), status: .delivered)
            ),
            ChatCallMessageConfiguration.outbox(
                callType: .video,
                state: .missed,
                statusConfiguration: .outbox(date: .now, formatter: PreviewTimestampFormatter(), status: .delivered)
            ),
            ChatCallMessageConfiguration.outbox(
                callType: .audio,
                state: .cancelled,
                statusConfiguration: .outbox(date: .now, formatter: PreviewTimestampFormatter(), status: .delivered)
            ),
            ChatCallMessageConfiguration.outbox(
                callType: .video,
                state: .cancelled,
                statusConfiguration: .outbox(date: .now, formatter: PreviewTimestampFormatter(), status: .delivered)
            ),
        ]

        let msgs: [IdentifiableAnyContentConfiguration<String>] = messages.map {
            IdentifiableAnyContentConfiguration(UUID().uuidString, $0)
        }

        let layout = ChatViewLayout()
        let viewModel = ChatViewLayout.ViewModel(
            headerConfiguration: .empty(),
            chatInputConfiguration: nil,
            scrollDownConfiguration: .init(available: false, unreadCount: 0),
            sections: [.init(identifier: "Section 1", dateText: "Today", messages: msgs)],
            footerConfiguration: nil
        )
        layout.bind(viewModel: viewModel)

        let viewController = UIViewController()
        viewController.view.addSubview(layout)
        layout.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        return viewController
    }
#endif
