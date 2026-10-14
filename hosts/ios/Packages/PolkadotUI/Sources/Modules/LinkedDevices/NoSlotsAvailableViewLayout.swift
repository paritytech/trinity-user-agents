import DesignSystem
import UIKit
internal import UIKit_iOS
internal import SnapKit

public final class NoSlotsAvailableViewLayout: BottomSheetBaseLayout {
    private let titleLabel: Label = create {
        $0.typography = .headlineSmall
        $0.textColor = .fgPrimary
        $0.textAlignment = .center
        $0.numberOfLines = 0
        $0.text = String(localized: .linkedDevicesNoSlotsTitle)
    }

    private let descriptionLabel: Label = create {
        $0.typography = .paragraphLarge
        $0.textColor = .fgPrimary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    private let dismissButtonView: RoundedButton = .create { button in
        button.applySecondaryStyle()
        button.setTitle(String(localized: .linkedDevicesNoSlotsClose))
    }

    public var dismissButton: UIControl {
        dismissButtonView
    }

    public func setDescription(_ text: String) {
        descriptionLabel.text = text
    }

    override public func setupLayout() {
        super.setupLayout()

        contentView.addSubview(titleLabel)
        contentView.addSubview(descriptionLabel)
        contentView.addSubview(dismissButtonView)

        titleLabel.snp.makeConstraints { make in
            make.top.equalToSuperview().offset(16)
            make.leading.trailing.equalToSuperview()
        }

        descriptionLabel.snp.makeConstraints { make in
            make.top.equalTo(titleLabel.snp.bottom).offset(32)
            make.leading.trailing.equalToSuperview()
        }

        dismissButtonView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.bottom.equalToSuperview().inset(16)
            make.top.equalTo(descriptionLabel.snp.bottom).offset(32)
            make.height.equalTo(UIConstants.actionHeight)
        }
    }
}
