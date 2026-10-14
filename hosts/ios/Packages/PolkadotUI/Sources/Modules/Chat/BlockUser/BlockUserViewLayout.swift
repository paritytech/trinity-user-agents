import UIKit
import DesignSystem
internal import UIKit_iOS
internal import SnapKit

public final class BlockUserViewLayout: BottomSheetBaseLayout {
    private let titleView: TopBottomLabelView = .create { view in
        view.topLabel.typography = .headlineSmall
        view.topLabel.textColor = .fgPrimary
        view.topLabel.textAlignment = .center
        view.topLabel.numberOfLines = 0

        view.bottomLabel.typography = .paragraphLarge
        view.bottomLabel.textColor = .fgTertiary
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

    public var blockButton: UIControl {
        actionView.fView
    }

    public var cancelButton: UIControl {
        actionView.sView
    }

    override public func setupLayout() {
        super.setupLayout()
        contentView.addSubview(titleView)
        contentView.addSubview(actionView)

        titleView.snp.makeConstraints { make in
            make.top.equalToSuperview()
            make.leading.trailing.equalToSuperview()
        }
        actionView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.bottom.equalToSuperview()
            make.top.equalTo(titleView.snp.bottom).offset(32)
        }
    }

    public func bind(username: String) {
        titleView.topLabel.text = String(localized: .blockUserTitle(username: username))
        titleView.bottomLabel.text = String(localized: .blockUserMessage)
        actionView.fView.setTitle(String(localized: .blockUserConfirm))
        actionView.sView.setTitle(String(localized: .Common.cancel))
    }
}
