import UIKit
internal import SnapKit

public struct ChatSystemMessageConfiguration: HashableContentConfiguration {
    let contentProvider: any HashableContentConfiguration
    let backgroundConfiguration: BackgroundConfiguration
    let contentInsets: NSDirectionalEdgeInsets

    init(
        contentProvider: any HashableContentConfiguration,
        textBackgroundConfiguration: BackgroundConfiguration = .empty,
        contentInsets: NSDirectionalEdgeInsets = .zero
    ) {
        self.contentProvider = contentProvider
        backgroundConfiguration = textBackgroundConfiguration
        self.contentInsets = contentInsets
    }

    public func makeContentView() -> any UIView & UIContentView {
        ChatSystemMessageView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }

    public static func == (
        lhs: ChatSystemMessageConfiguration,
        rhs: ChatSystemMessageConfiguration
    ) -> Bool {
        lhs.contentProvider.hashValue == rhs.contentProvider.hashValue &&
            lhs.contentInsets == rhs.contentInsets &&
            lhs.backgroundConfiguration.hashValue == rhs.backgroundConfiguration.hashValue
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(contentProvider)
        hasher.combine(contentInsets)
        hasher.combine(backgroundConfiguration)
    }
}

extension ChatSystemMessageConfiguration {
    struct BackgroundConfiguration: Hashable {
        let color: UIColor?
        let cornerRadius: CGFloat
        let insets: NSDirectionalEdgeInsets

        static var empty: Self {
            .init(color: .clear, cornerRadius: 0, insets: .zero)
        }
    }
}

final class ChatSystemMessageView: UIView, UIContentView {
    typealias Configuration = ChatSystemMessageConfiguration

    private let contentContainer = UIView()
    private var appliedConfiguration: Configuration
    private var contentView: (UIView & UIContentView)?

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: Configuration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        addSubview(contentContainer)
        contentContainer.snp.makeConstraints {
            $0.leading.equalToSuperview()
                .offset(appliedConfiguration.contentInsets.leading)
            $0.trailing.equalToSuperview()
                .inset(appliedConfiguration.contentInsets.trailing)
            $0.top.equalToSuperview()
                .offset(appliedConfiguration.contentInsets.top)
            $0.bottom.equalToSuperview()
                .inset(appliedConfiguration.contentInsets.bottom)
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? Configuration else { return }
        appliedConfiguration = configuration

        contentContainer.backgroundColor = configuration.backgroundConfiguration.color
        contentContainer.layer.cornerRadius = configuration.backgroundConfiguration.cornerRadius

        contentContainer.snp.updateConstraints {
            $0.leading.equalToSuperview()
                .offset(appliedConfiguration.contentInsets.leading)
            $0.trailing.equalToSuperview()
                .inset(appliedConfiguration.contentInsets.trailing)
            $0.top.equalToSuperview()
                .offset(appliedConfiguration.contentInsets.top)
            $0.bottom.equalToSuperview()
                .inset(appliedConfiguration.contentInsets.bottom)
        }

        let contentConfiguration = appliedConfiguration.contentProvider

        contentView.apply(contentConfiguration) { addContent($0) }
    }

    private func addContent(_ contentView: UIView) {
        contentContainer.addSubview(contentView)
        contentView.snp.remakeConstraints {
            $0.leading.equalToSuperview()
                .offset(appliedConfiguration.backgroundConfiguration.insets.leading)
            $0.trailing.equalToSuperview()
                .inset(appliedConfiguration.backgroundConfiguration.insets.trailing)
            $0.top.equalToSuperview()
                .offset(appliedConfiguration.backgroundConfiguration.insets.top)
            $0.bottom.equalToSuperview()
                .inset(appliedConfiguration.backgroundConfiguration.insets.bottom)
        }
    }
}

#Preview(traits: .fixedLayout(width: 300, height: 150)) {
    let action = ChatMessageActionView(viewModel: ChatMessageActionView.ViewModel(
        title: "John Doe",
        subtitle: "Attest humans, remove AI in a 5 minute game",
        buttonTitle: "Open",
        buttonAction: {
            print("View Profile tapped!")
        }
    ))

    let bgConfig = ChatSystemMessageConfiguration.BackgroundConfiguration(
        color: UIColor.lightGray,
        cornerRadius: 24,
        insets: .all(insets: 12)
    )

    return ChatSystemMessageConfiguration(
        contentProvider: SwiftUIContentConfiguration(view: action),
        textBackgroundConfiguration: bgConfig,
        contentInsets: .zero
    )
    .makeContentView()
}
