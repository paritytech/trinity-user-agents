import UIKit
import DesignSystem
internal import SnapKit

public struct SelectableContactConfiguration: HashableContentConfiguration {
    public let avatar: AvatarViewModel
    public let name: String
    public let isSelected: Bool
    public let onSelection: ((Bool) -> Void)?

    public init(
        avatar: AvatarViewModel,
        name: String,
        isSelected: Bool,
        onSelection: ((Bool) -> Void)? = nil
    ) {
        self.avatar = avatar
        self.name = name
        self.isSelected = isSelected
        self.onSelection = onSelection
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(avatar)
        hasher.combine(name)
        hasher.combine(isSelected)
    }

    public static func == (lhs: SelectableContactConfiguration, rhs: SelectableContactConfiguration) -> Bool {
        lhs.avatar == rhs.avatar
            && lhs.name == rhs.name
            && lhs.isSelected == rhs.isSelected
    }

    public func makeContentView() -> any UIView & UIContentView {
        SelectableContactView(configuration: self)
    }
}

public final class SelectableContactView: UIView, UIContentView {
    private let selectableAvatarView = SelectableAvatarView()

    private let nameLabel: Label = .create { label in
        label.typography = .paragraphSmall
        label.textColor = .fgPrimary
        label.textAlignment = .center
        label.numberOfLines = 1
        label.adjustsFontSizeToFitWidth = true
        label.minimumScaleFactor = 0.8
    }

    private var appliedConfiguration: SelectableContactConfiguration

    public var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    public init(configuration: SelectableContactConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        addSubview(selectableAvatarView)
        selectableAvatarView.snp.makeConstraints {
            $0.size.equalTo(selectableAvatarView.proposedDimension)
            $0.top.equalToSuperview()
            $0.centerX.equalToSuperview()
        }

        addSubview(nameLabel)
        nameLabel.snp.makeConstraints {
            $0.top.equalTo(selectableAvatarView.snp.bottom).offset(5)
            $0.leading.trailing.equalToSuperview()
            $0.bottom.lessThanOrEqualToSuperview()
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? SelectableContactConfiguration else { return }
        appliedConfiguration = configuration

        selectableAvatarView.configure(
            with: SelectableAvatarConfiguration(
                avatar: configuration.avatar,
                isSelected: configuration.isSelected
            )
        )
        nameLabel.text = configuration.name
    }

    public func notifyTapped() {
        appliedConfiguration.onSelection?(!appliedConfiguration.isSelected)
    }
}
