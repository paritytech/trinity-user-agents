import UIKit
import UIKit_iOS
import PolkadotUI
import DesignSystem

final class LeaveChatViewLayout: BottomSheetBaseLayout {
    private let titleView: TopBottomLabelView = .create { view in
        view.topLabel.typography = .headlineSmall
        view.topLabel.textColor = .fgPrimary
        view.topLabel.textAlignment = .center
        view.topLabel.numberOfLines = 0

        view.bottomLabel.typography = .paragraphLarge
        view.bottomLabel.textColor = .fgSecondary
        view.bottomLabel.textAlignment = .center
        view.bottomLabel.numberOfLines = 0

        view.spacing = 8
    }

    private let actionView: GenericPairValueView<
        DSButtonView,
        DSButtonView
    > = .create { view in
        view.fView.style = .destructive
        view.fView.size = .mediumIncreased
        view.fView.expands = true
        view.sView.style = .tertiary
        view.sView.size = .mediumIncreased
        view.sView.expands = true

        view.setVerticalAndSpacing(view.fView.size.verticalPadding)
        view.stackView.distribution = .fillEqually
    }

    private var titleLabel: UILabel {
        titleView.topLabel
    }

    private var descriptionLabel: UILabel {
        titleView.bottomLabel
    }

    var deleteButton: DSButtonView {
        actionView.fView
    }

    var cancelButton: DSButtonView {
        actionView.sView
    }

    override func setupLayout() {
        super.setupLayout()
        contentView.addSubview(titleView)
        contentView.addSubview(actionView)

        titleView.snp.makeConstraints { make in
            make.top.equalToSuperview().offset(-16)
            make.leading.trailing.equalToSuperview()
        }
        actionView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.bottom.equalToSuperview().offset(8)
            make.top.equalTo(titleView.snp.bottom).offset(32)
        }
    }

    func bind(username: String) {
        titleLabel.text = String(localized: .leaveChatTitle)
        descriptionLabel.text = String(localized: .leaveChatMessage(username: username))
        deleteButton.setTitle(String(localized: .leaveChatDelete))
        cancelButton.setTitle(String(localized: .Common.cancel))
    }
}
