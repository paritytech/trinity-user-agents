import UIKit
import UIKit_iOS
import PolkadotUI

struct TransferPrivacyViewModel {
    let title: String
    let message: String
    let sendAnywayTitle: String
}

final class TransferPrivacyViewLayout: BottomSheetBaseLayout {
    private let iconView: UIImageView = .create { view in
        view.image = UIImage(resource: .iconInfo60)
        view.setContentCompressionResistancePriority(.required, for: .vertical)
    }

    private let titleLabel: UILabel = .create { label in
        label.textColor = .fgPrimary
        label.font = .semibold24
        label.numberOfLines = 0
        label.textAlignment = .center
    }

    private let messageLabel: UILabel = .create { label in
        label.textColor = .fgSecondary
        label.font = .regular16
        label.numberOfLines = 0
        label.textAlignment = .center
    }

    let mainButton: RoundedButton = .create { button in
        button.applyMainStyle()
    }

    let cancelButton: UIButton = .create { button in
        button.setTitleColor(.fgTertiary, for: .normal)
        button.titleLabel?.font = .semibold16
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func setupLayout() {
        super.setupLayout()
        backgroundView.fillColor = .clear
        backgroundColor = .bgSurfaceContainer

        let textsStack: UIStackView = .create { stack in
            stack.axis = .vertical
            stack.spacing = 12
            stack.alignment = .center
        }
        textsStack.addArrangedSubview(titleLabel)
        textsStack.addArrangedSubview(messageLabel)

        let contentStack: UIStackView = .create { stack in
            stack.axis = .vertical
            stack.spacing = 16
            stack.alignment = .center
        }
        contentStack.addArrangedSubview(iconView)
        contentStack.addArrangedSubview(textsStack)

        let actionsStack: UIStackView = .create { stack in
            stack.axis = .vertical
            stack.spacing = 8
        }
        actionsStack.addArrangedSubview(mainButton)
        actionsStack.addArrangedSubview(cancelButton)

        contentView.addSubview(contentStack)
        contentView.addSubview(actionsStack)

        contentStack.snp.makeConstraints { make in
            make.leading.trailing.top.equalToSuperview()
        }

        actionsStack.snp.makeConstraints { make in
            make.leading.trailing.bottom.equalToSuperview()
            make.top.equalTo(contentStack.snp.bottom).offset(32)
        }

        mainButton.snp.makeConstraints { make in
            make.height.equalTo(UIConstants.actionHeight)
        }

        cancelButton.snp.makeConstraints { make in
            make.height.equalTo(52)
        }
    }
}

extension TransferPrivacyViewLayout {
    func bind(viewModel: TransferPrivacyViewModel) {
        titleLabel.text = viewModel.title
        messageLabel.text = viewModel.message
        mainButton.imageWithTitleView?.title = viewModel.sendAnywayTitle
        cancelButton.setTitle(String(localized: .Common.cancel), for: .normal)
    }
}
