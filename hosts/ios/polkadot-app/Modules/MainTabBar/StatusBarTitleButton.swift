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

    init() {
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
}

private extension StatusBarTitleButton {
    enum Constants {
        static let logoSize: CGFloat = 16
        static let logoSpacing: CGFloat = 10
    }

    func setupLayout() {
        logoView.snp.makeConstraints { make in
            make.size.equalTo(Constants.logoSize)
        }

        let stackView = UIStackView(arrangedSubviews: [logoView, nameLabel])
        stackView.axis = .horizontal
        stackView.alignment = .center
        stackView.spacing = Constants.logoSpacing
        stackView.isUserInteractionEnabled = false

        addSubview(stackView)
        stackView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }
}
