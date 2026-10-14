import DesignSystem
import ExternalAccessibility
import UIKit
internal import SnapKit

public struct SearchContactListConfiguration: HashableContentConfiguration {
    public var userName: String
    public var avatarViewModel: AvatarViewModel

    public init(userName: String, avatarViewModel: AvatarViewModel) {
        self.userName = userName
        self.avatarViewModel = avatarViewModel
    }

    public func makeContentView() -> any UIView & UIContentView {
        SearchContactListView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }
}

final class SearchContactListView: UIView, UIContentView {
    let avatarView = DSAvatarView(size: .s40)

    let nameLabel: Label = create {
        $0.typography = .titleMedium
        $0.textColor = UIColor.fgPrimary
        $0.numberOfLines = 1
    }

    private var appliedConfiguration: SearchContactListConfiguration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: SearchContactListConfiguration) {
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
        addSubview(avatarView)
        avatarView.snp.makeConstraints {
            $0.size.equalTo(avatarView.proposedDimension)
            $0.leading.equalToSuperview()
            $0.top.bottom.equalToSuperview()
        }

        addSubview(nameLabel)
        nameLabel.snp.makeConstraints {
            $0.leading.equalTo(avatarView.snp.trailing).offset(16)
            $0.trailing.equalToSuperview().inset(16)
            $0.centerY.equalToSuperview()
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? SearchContactListConfiguration else { return }
        appliedConfiguration = configuration

        nameLabel.text = configuration.userName
        avatarView.viewModel = configuration.avatarViewModel
    }
}

#Preview(
//    traits: .fixedLayout(width: 358, height: 72)
) {
    SearchContactListConfiguration(
        userName: "Jake.23",
        avatarViewModel: .colored(text: "J", colorSeed: "preview")
    ).makeContentView()
}

// MARK: - AccessibilityBound

extension SearchContactListView: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [.init(self, AccessibilityID.Chats.searchResultRow)]
    }
}
