import UIKit
import UIKit_iOS
import PolkadotUI

final class TransactionSuccessViewLayout: UIView {
    let iconView: GenericBorderedView<UIImageView> = .create { view in
        view.backgroundView.applyBackgroundStyle(.bgStatusSuccess, cornerRadius: 40)
        view.contentInsets = UIEdgeInsets(top: 20, left: 20, bottom: 20, right: 20)
        view.contentView.image = .tick40.withRenderingMode(.alwaysTemplate)
        view.contentView.tintColor = .fgStaticWhite
    }

    let titleLabel: Label = .create { view in
        view.typography = .headlineSmall
        view.textColor = .fgPrimary
        view.numberOfLines = 0
        view.textAlignment = .center
    }

    let doneButton = DSButtonView("", size: .large, expands: true)

    override init(frame: CGRect) {
        super.init(frame: frame)

        backgroundColor = .bgSurfaceMain

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupLayout() {
        let contentView = UIView.vStack(alignment: .center, spacing: 16, [iconView, titleLabel])

        addSubview(contentView)
        contentView.snp.makeConstraints { make in
            make.trailing.leading.equalToSuperview().inset(UIConstants.horizontalInsetMedium)
            make.centerY.equalToSuperview()
        }

        iconView.snp.makeConstraints { make in
            make.height.width.equalTo(2 * iconView.backgroundView.cornerRadius)
        }

        addSubview(doneButton)
        doneButton.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(UIConstants.horizontalInsetWide)
            make.bottom.equalTo(safeAreaLayoutGuide.snp.bottom).offset(-20)
            make.height.equalTo(doneButton.proposedHeight)
        }
    }
}
