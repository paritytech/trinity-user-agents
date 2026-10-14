import UIKit
import DesignSystem
internal import UIKit_iOS
internal import SnapKit

public final class AddContactViewLayout: BottomSheetBaseLayout {
    private let avatarView = DSAvatarView(size: .s72)

    private let titleLabel: Label = create {
        $0.typography = .headlineSmall
        $0.textColor = .fgPrimary
        $0.numberOfLines = 0
        $0.textAlignment = .center
    }

    private let addToContactsLoadableButton = DSLoadableButtonView(
        contentView: DSButtonView(
            String(localized: .addToContactsAction),
            size: .mediumIncreased,
            expands: true
        )
    )

    private let detailsView: GenericPairValueView<Label, UIImageView> = create {
        $0.spacing = 24
        $0.stackView.axis = .horizontal
        $0.stackView.alignment = .center

        $0.fView.typography = .bodyMedium
        $0.fView.textColor = .fgTertiary
        $0.fView.numberOfLines = 0

        $0.sView.image = UIImage(resource: .info28)
        $0.sView.setContentHuggingPriority(.required, for: .horizontal)
        $0.sView.setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    private var detailsLabel: Label {
        detailsView.fView
    }

    public var addContactHandler: (() -> Void)?

    public private(set) var activityInProgress: Bool = false

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setupViews() {
        addToContactsLoadableButton.contentView.onTap = { [weak self] in
            self?.addContactHandler?()
        }

        addSubview(avatarView)
        addSubview(titleLabel)
        addSubview(addToContactsLoadableButton)
        addSubview(detailsView)

        avatarView.snp.makeConstraints {
            $0.size.equalTo(avatarView.proposedDimension)
            $0.centerX.equalToSuperview()
            $0.top.equalToSuperview().offset(40)
        }

        titleLabel.snp.makeConstraints {
            $0.top.equalTo(avatarView.snp.bottom).offset(20)
            $0.directionalHorizontalEdges.equalToSuperview().inset(48)
        }

        addToContactsLoadableButton.snp.makeConstraints {
            $0.top.equalTo(titleLabel.snp.bottom).offset(40)
            $0.directionalHorizontalEdges.equalToSuperview().inset(32)
            $0.height.equalTo(addToContactsLoadableButton.contentView.proposedHeight)
        }

        detailsView.snp.makeConstraints {
            $0.top.equalTo(addToContactsLoadableButton.snp.bottom).offset(40)
            $0.directionalHorizontalEdges.equalToSuperview().inset(32)
            $0.bottom.equalToSuperview().inset(32)
        }
    }
}

public extension AddContactViewLayout {
    struct ViewModel {
        let username: String
        let avatarViewModel: AvatarViewModel

        public init(username: String, avatarViewModel: AvatarViewModel) {
            self.username = username
            self.avatarViewModel = avatarViewModel
        }
    }

    func bind(viewModel: ViewModel) {
        avatarView.viewModel = viewModel.avatarViewModel
        titleLabel.text = String(
            localized: .addToContactsTitle(username: viewModel.username)
        )
        detailsLabel.text = String(localized: .addToContactsDetails(username: viewModel.username))
    }

    func showActivity(active: Bool) {
        activityInProgress = active

        if active {
            addToContactsLoadableButton.startLoading()
        } else {
            addToContactsLoadableButton.stopLoading()
        }
    }
}

#Preview(traits: .fixedLayout(width: 400, height: 500)) {
    let layout = AddContactViewLayout()
    let viewModel = AddContactViewLayout.ViewModel(
        username: "juliuslongname.87",
        avatarViewModel: .colored(text: "J", colorSeed: "test")
    )
    layout.bind(viewModel: viewModel)
    return layout
}
