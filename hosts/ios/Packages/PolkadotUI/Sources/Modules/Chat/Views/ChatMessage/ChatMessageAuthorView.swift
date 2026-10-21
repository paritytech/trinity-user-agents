import DesignSystem
import UIKit
internal import SnapKit

/// The participant that wrote a message, in a room with many participants (Chat Extension v2, P6.3).
public struct ChatMessageAuthor: Hashable {
    public let participantId: String
    public let name: String
    public let avatar: AvatarViewModel
    /// Opens the sheet of the participant. It takes no part in the equality.
    public let onTap: (() -> Void)?

    public init(participantId: String, name: String, avatar: AvatarViewModel, onTap: (() -> Void)?) {
        self.participantId = participantId
        self.name = name
        self.avatar = avatar
        self.onTap = onTap
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.participantId == rhs.participantId && lhs.name == rhs.name && lhs.avatar == rhs.avatar
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(participantId)
        hasher.combine(name)
        hasher.combine(avatar)
    }
}

/// A small avatar and the name of the author, above the bubble.
final class ChatMessageAuthorView: UIControl {
    private let avatarView = DSAvatarView(size: .s20)
    private let nameLabel: Label = create {
        $0.typography = .captionMedium
        $0.textColor = .fgSecondary
        $0.numberOfLines = 1
        $0.lineBreakMode = .byTruncatingTail
    }

    private var onTap: (() -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)

        addSubview(avatarView)
        addSubview(nameLabel)
        avatarView.isUserInteractionEnabled = false
        nameLabel.isUserInteractionEnabled = false

        avatarView.snp.makeConstraints {
            $0.leading.top.bottom.equalToSuperview()
            $0.size.equalTo(20)
        }
        nameLabel.snp.makeConstraints {
            $0.leading.equalTo(avatarView.snp.trailing).offset(DSSpacings.extraSmall)
            $0.trailing.equalToSuperview()
            $0.centerY.equalTo(avatarView)
        }

        addTarget(self, action: #selector(handleTap), for: .touchUpInside)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(_ author: ChatMessageAuthor) {
        avatarView.viewModel = author.avatar
        nameLabel.text = author.name
        onTap = author.onTap
        accessibilityLabel = author.name
    }

    @objc private func handleTap() {
        onTap?()
    }
}
