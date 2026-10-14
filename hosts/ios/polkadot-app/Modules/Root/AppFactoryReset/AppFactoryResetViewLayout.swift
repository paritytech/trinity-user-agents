#if TESTNET_FEATURE
    import UIKit
    import UIKit_iOS
    import SnapKit
    import PolkadotUI
    import DesignSystem

    final class AppFactoryResetViewLayout: BottomSheetBaseLayout {
        let titleLabel: Label = .create {
            $0.numberOfLines = 0
            $0.typography = .headlineSmall
            $0.textColor = .fgPrimary
            $0.textAlignment = .left
            $0.text = String(localized: .Common.appFactoryResetTitle)
        }

        let descriptionLabel: Label = .create {
            $0.numberOfLines = 0
            $0.typography = .paragraphLarge
            $0.textColor = .fgTertiary
            $0.textAlignment = .left
            $0.text = String(localized: .Common.appFactoryResetDescription)
        }

        let startOverButton: RoundedButton = .create { button in
            button.applyDestructiveStyle()
            button.imageWithTitleView?.titleFont = .semibold16
            button.setTitle(String(localized: .Common.appFactoryResetStartOver))
        }

        let dismissButton: RoundedButton = .create { button in
            button.applyTitleTertiaryStyle()
            button.imageWithTitleView?.titleFont = .semibold16
            button.setTitle(String(localized: .Common.appFactoryResetDismiss))
        }

        override func setupLayout() {
            super.setupLayout()

            contentView.addSubview(titleLabel)
            titleLabel.snp.makeConstraints { make in
                make.top.leading.trailing.equalToSuperview()
            }

            contentView.addSubview(descriptionLabel)
            descriptionLabel.snp.makeConstraints { make in
                make.top.equalTo(titleLabel.snp.bottom).offset(8)
                make.leading.trailing.equalToSuperview()
            }

            let buttonsStackView = UIStackView(arrangedSubviews: [startOverButton, dismissButton])
            buttonsStackView.axis = .vertical
            buttonsStackView.spacing = 8
            buttonsStackView.distribution = .fillEqually

            contentView.addSubview(buttonsStackView)
            buttonsStackView.snp.makeConstraints { make in
                make.top.equalTo(descriptionLabel.snp.bottom).offset(24)
                make.leading.trailing.equalToSuperview()
                make.bottom.equalToSuperview()
            }

            startOverButton.snp.makeConstraints { make in
                make.height.equalTo(UIConstants.actionHeight)
            }

            dismissButton.snp.makeConstraints { make in
                make.height.equalTo(UIConstants.actionHeight)
            }
        }
    }
#endif
