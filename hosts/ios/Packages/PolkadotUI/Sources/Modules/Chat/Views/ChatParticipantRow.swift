import DesignSystem
import UIKit
internal import SnapKit

/// One participant of a product room in the participant list: the avatar, the name and the kind.
public struct ChatParticipantRowViewModel: Hashable {
    public let participantId: String
    public let name: String
    public let detail: String
    public let avatar: AvatarViewModel
    /// Loaded over ``avatar``, which shows until it arrives. It takes no part in the equality.
    public let icon: (any ImageViewModelProtocol)?

    public init(
        participantId: String,
        name: String,
        detail: String,
        avatar: AvatarViewModel,
        icon: (any ImageViewModelProtocol)?
    ) {
        self.participantId = participantId
        self.name = name
        self.detail = detail
        self.avatar = avatar
        self.icon = icon
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.participantId == rhs.participantId && lhs.name == rhs.name && lhs.detail == rhs.detail
            && lhs.avatar == rhs.avatar
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(participantId)
        hasher.combine(name)
        hasher.combine(detail)
    }
}

public final class ChatParticipantRow: UITableViewCell {
    public static let cellIdentifier = "ChatParticipantRow"

    private let avatarView = DSAvatarView(size: .s40)
    private let iconView = UIImageView()
    private let nameLabel: Label = create {
        $0.typography = .paragraphLarge
        $0.textColor = .fgPrimary
        $0.numberOfLines = 1
    }

    private let detailLabel: Label = create {
        $0.typography = .captionMedium
        $0.textColor = .fgSecondary
        $0.numberOfLines = 1
    }

    private var icon: (any ImageViewModelProtocol)?

    override public init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
        super.init(style: style, reuseIdentifier: reuseIdentifier)
        backgroundColor = .clear
        setupViews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func prepareForReuse() {
        super.prepareForReuse()
        icon?.cancel(on: iconView)
        iconView.image = nil
    }

    public func bind(_ viewModel: ChatParticipantRowViewModel) {
        avatarView.viewModel = viewModel.avatar
        nameLabel.text = viewModel.name
        detailLabel.text = viewModel.detail
        detailLabel.isHidden = viewModel.detail.isEmpty
        icon = viewModel.icon
        viewModel.icon?.loadImage(on: iconView, targetSize: CGSize(width: 40, height: 40), animated: true)
    }

    private func setupViews() {
        iconView.contentMode = .scaleAspectFill
        iconView.clipsToBounds = true
        iconView.layer.cornerRadius = 20

        let labels = UIStackView(arrangedSubviews: [nameLabel, detailLabel])
        labels.axis = .vertical
        labels.spacing = DSSpacings.extraTiny

        contentView.addSubview(avatarView)
        contentView.addSubview(iconView)
        contentView.addSubview(labels)

        avatarView.snp.makeConstraints {
            $0.leading.equalToSuperview().inset(DSSpacings.medium)
            $0.top.bottom.equalToSuperview().inset(DSSpacings.small)
            $0.size.equalTo(40)
        }
        iconView.snp.makeConstraints { $0.edges.equalTo(avatarView) }
        labels.snp.makeConstraints {
            $0.leading.equalTo(avatarView.snp.trailing).offset(DSSpacings.small)
            $0.trailing.equalToSuperview().inset(DSSpacings.medium)
            $0.centerY.equalTo(avatarView)
        }
    }
}
