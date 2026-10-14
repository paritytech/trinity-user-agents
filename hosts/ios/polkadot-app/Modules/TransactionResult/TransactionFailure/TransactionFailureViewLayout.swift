import UIKit
import UIKit_iOS
import PolkadotUI

final class TransactionFailureViewLayout: UIView {
    let iconView: GenericBorderedView<UIImageView> = .create { view in
        view.backgroundView.applyBackgroundStyle(.bgStatusError, cornerRadius: 40)
        view.contentInsets = UIEdgeInsets(top: 20, left: 20, bottom: 20, right: 20)
        view.contentView.image = .cross40
    }

    let titleLabel: Label = .create { view in
        view.typography = .headlineSmall
        view.textColor = .fgPrimary
        view.numberOfLines = 0
        view.textAlignment = .center
    }

    let detailsLabel: Label = .create { view in
        view.typography = .bodyMedium
        view.textColor = .fgSecondary
        view.numberOfLines = 0
        view.textAlignment = .center
    }

    let actionButton = DSButtonView("", size: .large, expands: true)

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
        let contentView = UIView.vStack(alignment: .center, spacing: 0, [iconView])
        contentView.setCustomSpacing(16, after: iconView)

        contentView.addArrangedSubview(titleLabel)
        contentView.setCustomSpacing(12, after: titleLabel)

        contentView.addArrangedSubview(detailsLabel)

        addSubview(contentView)
        contentView.snp.makeConstraints { make in
            make.trailing.leading.equalToSuperview().inset(UIConstants.horizontalInsetWide)
            make.centerY.equalToSuperview()
        }

        iconView.snp.makeConstraints { make in
            make.height.width.equalTo(2 * iconView.backgroundView.cornerRadius)
        }

        addSubview(actionButton)
        actionButton.snp.makeConstraints { make in
            make.centerX.equalToSuperview()
            make.leading.equalToSuperview().inset(UIConstants.horizontalInsetWide)
            make.bottom.equalTo(safeAreaLayoutGuide.snp.bottom).offset(-20)
            make.height.equalTo(actionButton.proposedHeight)
        }
    }
}
