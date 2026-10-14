import DesignSystem
import UIKit
internal import UIKit_iOS
internal import SnapKit

public final class RemoveDeviceViewLayout: BottomSheetBaseLayout {
    private let titleLabel: Label = create {
        $0.typography = .headlineSmall
        $0.textColor = .fgPrimary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    private let iconImageView: UIImageView = {
        let view = UIImageView()
        view.contentMode = .scaleAspectFit
        view.tintColor = .fgPrimary
        view.image = UIImage(resource: .linkedDeviceMonitor)
        return view
    }()

    private let descriptionLabel: Label = create {
        $0.typography = .paragraphLarge
        $0.textColor = .fgPrimary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    private let actionView: GenericPairValueView<
        RoundedButton,
        LoadableRoundedButton
    > = .create { view in
        view.fView.applySecondaryStyle()
        view.sView.contentView.applyDestructiveStyle()

        view.setHorizontalAndSpacing(8)
        view.stackView.distribution = .fillEqually
    }

    public var cancelButton: UIControl {
        actionView.fView
    }

    public var removeButton: UIControl {
        actionView.sView.contentView
    }

    override public func setupLayout() {
        super.setupLayout()

        contentView.addSubview(titleLabel)
        contentView.addSubview(iconImageView)
        contentView.addSubview(descriptionLabel)
        contentView.addSubview(actionView)

        titleLabel.snp.makeConstraints { make in
            make.top.equalToSuperview().offset(16)
            make.leading.trailing.equalToSuperview()
        }

        iconImageView.snp.makeConstraints { make in
            make.top.equalTo(titleLabel.snp.bottom).offset(32)
            make.centerX.equalToSuperview()
            make.height.equalTo(68)
            make.width.equalTo(80)
        }

        descriptionLabel.snp.makeConstraints { make in
            make.top.equalTo(iconImageView.snp.bottom).offset(16)
            make.leading.trailing.equalToSuperview()
        }

        actionView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.bottom.equalToSuperview().inset(16)
            make.top.equalTo(descriptionLabel.snp.bottom).offset(32)
            make.height.equalTo(UIConstants.actionHeight)
        }
    }

    public func bind(deviceDescription: String) {
        titleLabel.text = String(localized: .linkedDevicesRemoveDeviceTitle)
        descriptionLabel.text = String(localized: .linkedDevicesRemoveDeviceMessage(device: deviceDescription))
        actionView.fView.setTitle(String(localized: .Common.cancel))
        actionView.sView.contentView.setTitle(String(localized: .linkedDevicesRemoveDeviceAction))
    }

    public func setLoading(_ loading: Bool) {
        if loading {
            actionView.fView.isEnabled = false
            actionView.sView.startLoading()
        } else {
            actionView.sView.stopLoading()
            actionView.fView.isEnabled = true
        }
    }
}
