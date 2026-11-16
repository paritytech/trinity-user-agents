import UIKit
import DesignSystem
import PolkadotUI
import SnapKit

final class StatusBarTitleButton: UIButton {
    private let logoView: UIImageView = .create {
        $0.image = UIImage(resource: .polkadotLogoLoading).withRenderingMode(.alwaysTemplate)
        $0.tintColor = .fgPrimary
        $0.contentMode = .scaleAspectFit
    }

    private let nameLabel: Label = .create {
        $0.typography = .titleSmall
        $0.textColor = .fgPrimary
    }

    private let menuAnchorView: UIView = .create {
        $0.backgroundColor = .clear
        $0.isUserInteractionEnabled = false
    }

    private let contentStackView: UIStackView

    private var isMenuVisible = false {
        didSet { updateContentAlpha() }
    }

    override var isHighlighted: Bool {
        didSet { updateContentAlpha() }
    }

    init() {
        contentStackView = UIStackView(arrangedSubviews: [logoView, nameLabel])
        super.init(frame: .zero)
        setupLayout()
        showsMenuAsPrimaryAction = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func bind(viewModel: StatusBarTitleViewModel) {
        nameLabel.text = viewModel.title
        logoView.isHidden = !viewModel.showsLogo
        menu = viewModel.menu
        isUserInteractionEnabled = viewModel.menu != nil
        accessibilityLabel = viewModel.title
        accessibilityTraits = viewModel.menu == nil ? .staticText : .button
    }

    override func contextMenuInteraction(
        _: UIContextMenuInteraction,
        configuration _: UIContextMenuConfiguration,
        highlightPreviewForItemWithIdentifier _: any NSCopying
    ) -> UITargetedPreview? {
        makeMenuAnchorPreview()
    }

    override func contextMenuInteraction(
        _: UIContextMenuInteraction,
        configuration _: UIContextMenuConfiguration,
        dismissalPreviewForItemWithIdentifier _: any NSCopying
    ) -> UITargetedPreview? {
        makeMenuAnchorPreview()
    }

    override func contextMenuInteraction(
        _ interaction: UIContextMenuInteraction,
        willDisplayMenuFor configuration: UIContextMenuConfiguration,
        animator: (any UIContextMenuInteractionAnimating)?
    ) {
        super.contextMenuInteraction(interaction, willDisplayMenuFor: configuration, animator: animator)
        isMenuVisible = true
    }

    override func contextMenuInteraction(
        _ interaction: UIContextMenuInteraction,
        willEndFor configuration: UIContextMenuConfiguration,
        animator: (any UIContextMenuInteractionAnimating)?
    ) {
        super.contextMenuInteraction(interaction, willEndFor: configuration, animator: animator)
        isMenuVisible = false
    }
}

private extension StatusBarTitleButton {
    enum Constants {
        static let logoSize: CGFloat = 16
        static let logoSpacing = DSSpacings.smallIncreased
        static let pressedAlpha: CGFloat = 0.5
    }

    func setupLayout() {
        logoView.snp.makeConstraints { make in
            make.size.equalTo(Constants.logoSize)
        }

        contentStackView.axis = .horizontal
        contentStackView.alignment = .center
        contentStackView.spacing = Constants.logoSpacing
        contentStackView.isUserInteractionEnabled = false

        addSubview(contentStackView)
        contentStackView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addSubview(menuAnchorView)
        menuAnchorView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.top.equalTo(snp.bottom)
            make.height.equalTo(1)
        }
    }

    func makeMenuAnchorPreview() -> UITargetedPreview {
        let parameters = UIPreviewParameters()
        parameters.backgroundColor = .clear
        return UITargetedPreview(view: menuAnchorView, parameters: parameters)
    }

    func updateContentAlpha() {
        contentStackView.alpha = isHighlighted || isMenuVisible ? Constants.pressedAlpha : 1
    }
}
