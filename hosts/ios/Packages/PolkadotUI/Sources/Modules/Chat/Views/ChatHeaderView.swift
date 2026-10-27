import DesignSystem
import SwiftUI
import UIKit
internal import SnapKit
internal import UIKit_iOS

public struct ChatHeaderConfiguration: HashableContentConfiguration {
    public let avatarViewModel: AvatarViewModel
    /// Loaded over ``avatarViewModel``, which shows until it arrives.
    public let icon: (any ImageViewModelProtocol)?
    public let username: String
    public let additionalInfo: String?

    public init(
        avatarViewModel: AvatarViewModel,
        icon: (any ImageViewModelProtocol)? = nil,
        username: String,
        additionalInfo: String? = nil
    ) {
        self.avatarViewModel = avatarViewModel
        self.icon = icon
        self.username = username
        self.additionalInfo = additionalInfo
    }

    public func makeContentView() -> any UIView & UIContentView {
        ChatHeaderView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }

    static func empty() -> Self {
        .init(avatarViewModel: .colored(text: "", colorSeed: ""), username: "")
    }

    public static func == (lhs: ChatHeaderConfiguration, rhs: ChatHeaderConfiguration) -> Bool {
        lhs.avatarViewModel == rhs.avatarViewModel &&
            lhs.username == rhs.username &&
            lhs.additionalInfo == rhs.additionalInfo
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(avatarViewModel)
        hasher.combine(username)
        hasher.combine(additionalInfo)
    }
}

public final class ChatHeaderView: UIView, UIContentView {
    private let avatarView = DSAvatarView(size: Layout.avatarSize)

    private let titleLabel: Label = create {
        $0.font = .titleLarge
        $0.numberOfLines = 1
        $0.lineBreakMode = .byTruncatingTail
        $0.textAlignment = .center
        $0.textColor = .fgPrimary
    }

    private lazy var contentStack: UIStackView = {
        let stack = UIStackView(arrangedSubviews: [avatarView, titleLabel])
        stack.axis = .horizontal
        stack.alignment = .center
        stack.spacing = DSSpacings.small
        return stack
    }()

    private(set) var appliedConfiguration: ChatHeaderConfiguration

    public var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: ChatHeaderConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension ChatHeaderView {
    func setupViews() {
        clipsToBounds = true

        titleLabel.setHidden(!Layout.showsTitle)

        addSubview(contentStack)
        contentStack.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }

    func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? ChatHeaderConfiguration else { return }
        appliedConfiguration = configuration

        avatarView.viewModel = configuration.avatarViewModel
        avatarView.icon = configuration.icon
        titleLabel.text = configuration.username
    }
}

private extension ChatHeaderView {
    enum Layout {
        static var isLiquidGlass: Bool {
            if #available(iOS 26.0, *) { true } else { false }
        }

        static var showsTitle: Bool { !isLiquidGlass }

        // Chat navigation bar avatar: iOS 26 leading (next to the back button),
        // iOS < 26 centered with the title. Sizes come from Figma.
        static var avatarSize: DSLetterAvatar.Size {
            isLiquidGlass ? .s44 : .s28
        }
    }
}

#Preview(traits: .fixedLayout(width: 300, height: 150)) {
    ChatHeaderConfiguration(
        avatarViewModel: .colored(text: "A", colorSeed: "preview"),
        username: "Alice",
        additionalInfo: "Subtitle"
    ).makeContentView()
}
