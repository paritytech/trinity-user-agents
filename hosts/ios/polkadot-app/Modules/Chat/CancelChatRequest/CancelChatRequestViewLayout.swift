import UIKit
import UIKit_iOS
import PolkadotUI
import DesignSystem

final class CancelChatRequestViewLayout: BottomSheetBaseLayout {
    private let titleView: TopBottomLabelView = .create { view in
        view.topLabel.typography = .headlineSmall
        view.topLabel.textColor = .fgPrimary
        view.topLabel.textAlignment = .center
        view.topLabel.numberOfLines = 0

        view.bottomLabel.typography = .paragraphLarge
        view.bottomLabel.textColor = .fgSecondary
        view.bottomLabel.textAlignment = .center
        view.bottomLabel.numberOfLines = 0

        view.spacing = DSSpacings.small
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

    var confirmButton: DSButtonView {
        actionView.fView
    }

    var keepButton: DSButtonView {
        actionView.sView
    }

    override func setupLayout() {
        super.setupLayout()
        contentView.addSubview(titleView)
        contentView.addSubview(actionView)

        titleView.snp.makeConstraints { make in
            make.top.equalToSuperview().offset(-DSSpacings.mediumIncreased)
            make.leading.trailing.equalToSuperview()
        }
        actionView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.bottom.equalToSuperview().offset(DSSpacings.small)
            make.top.equalTo(titleView.snp.bottom).offset(DSSpacings.extraLarge)
        }
    }

    func bind(username: String) {
        titleLabel.text = String(localized: .cancelChatRequestTitle)
        descriptionLabel.text = String(localized: .cancelChatRequestMessage(username: username))
        confirmButton.setTitle(String(localized: .cancelChatRequestConfirm))
        keepButton.setTitle(String(localized: .cancelChatRequestKeep))
    }
}
