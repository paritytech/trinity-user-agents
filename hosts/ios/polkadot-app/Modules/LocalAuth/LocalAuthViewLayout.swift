import UIKit
import UIKit_iOS
import PolkadotUI

final class LocalAuthViewLayout: UIView {
    let actionView: LoadableActionView = .create { (view: LoadableActionView) in
        view.applyMainStyle()
        view.actionButton.imageWithTitleView?.spacingBetweenLabelAndIcon = 8
        view.actionButton.setTitle(String(localized: .Common.retry).uppercased())
        view.actionButton.setIcon(.reload16)
    }

    var actionButton: RoundedButton {
        actionView.actionButton
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupLayout() {
        addSubview(actionView)

        actionView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(UIConstants.horizontalInsetWide)
            make.bottom.equalTo(safeAreaLayoutGuide).offset(-24)
            make.height.equalTo(UIConstants.actionHeight)
        }
    }
}
