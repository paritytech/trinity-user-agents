import UIKit
import UIKit_iOS
import SnapKit
import PolkadotUI
import DesignSystem

final class DebugSettingsViewLayout: UIView {
    let clearBackupButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.changesContentOpacityWhenHighlighted = false
        button.setTitle("Clear Backup")
    }

    let clearReferralButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.changesContentOpacityWhenHighlighted = false
        button.setTitle("Clear Referral")
    }

    let shareLogsButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.title = "Share Logs"
    }

    let productsButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.title = "Products"
    }

    let dotNsBrowserButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.title = "Open SPA"
    }

    let simulateCrash: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.title = "Simulate Crash"
    }

    let clearJWTTokenButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.title = "Clear JWT Token"
    }

    let replaceEntropyButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.title = "Replace Entropy (Random)"
    }

    let themeSelectionButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.title = "Theme Selection"
    }

    let resetTipsButton: RoundedButton = .create { button in
        button.applyMainStyle()
        button.imageWithTitleView?.title = "Reset Tips"
    }

    let strategyDebugSwitch = UISwitch()

    private let strategyDebugLabel: Label = .create { (view: Label) in
        view.typography = .bodyMedium
        view.textColor = .fgPrimary
        view.text = "Transfer Strategy Debug"
    }

    private let strategyDebugRow: UIStackView = .create { stack in
        stack.axis = .horizontal
        stack.alignment = .center
        stack.distribution = .equalSpacing
    }

    #if DEBUG
        let openTrUAPIPlaygroundButton: RoundedButton = .create { button in
            button.applyMainStyle()
            button.imageWithTitleView?.title = "Open TrUAPI Playground"
        }

        let pocketFacePreviewButton: RoundedButton = .create { button in
            button.applyMainStyle()
            button.imageWithTitleView?.title = "Pocket Face Preview"
        }

        let pocketCardsButton: RoundedButton = .create { button in
            button.applyMainStyle()
            button.imageWithTitleView?.title = "Pocket Cards"
        }
    #endif

    let truApiRuntimeSwitch = UISwitch()

    private let truApiRuntimeLabel: Label = .create { (view: Label) in
        view.typography = .bodyMedium
        view.textColor = .fgPrimary
        view.text = "TrUAPI Runtime"
    }

    private let truApiRuntimeRow: UIStackView = .create { stack in
        stack.axis = .horizontal
        stack.alignment = .center
        stack.distribution = .equalSpacing
    }

    let hostPlacedSwitch = UISwitch()

    private let hostPlacedLabel: Label = .create { (view: Label) in
        view.typography = .bodyMedium
        view.textColor = .fgPrimary
        view.text = "Host-placed products"
    }

    private let hostPlacedRow: UIStackView = .create { stack in
        stack.axis = .horizontal
        stack.alignment = .center
        stack.distribution = .equalSpacing
    }

    let chainLabel: Label = .create { (view: Label) in
        view.typography = .bodyMedium
        view.textColor = .fgSecondary
        view.lineBreakMode = .byTruncatingMiddle
        view.text = "Chain ID: \(AppConfig.Chains.usernameChain)"
    }

    private let scrollView: UIScrollView = .create { scrollView in
        scrollView.alwaysBounceVertical = true
        scrollView.showsVerticalScrollIndicator = true
    }

    private let stackView: UIStackView = .create { stackView in
        stackView.axis = .vertical
        stackView.spacing = 8
    }

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
        addSubview(scrollView)
        scrollView.snp.makeConstraints { make in
            make.top.bottom.equalTo(safeAreaLayoutGuide)
            make.leading.trailing.equalToSuperview()
        }

        scrollView.addSubview(stackView)
        stackView.snp.makeConstraints { make in
            make.top.equalTo(scrollView.contentLayoutGuide).offset(16)
            make.bottom.equalTo(scrollView.contentLayoutGuide).offset(-16)
            make.leading.trailing.equalTo(scrollView.frameLayoutGuide).inset(16)
        }

        strategyDebugRow.addArrangedSubviews([strategyDebugLabel, strategyDebugSwitch])
        truApiRuntimeRow.addArrangedSubviews([truApiRuntimeLabel, truApiRuntimeSwitch])
        hostPlacedRow.addArrangedSubviews([hostPlacedLabel, hostPlacedSwitch])

        var rows: [UIView] = [
            chainLabel,
            clearBackupButton,
            clearReferralButton,
            shareLogsButton,
            productsButton,
            dotNsBrowserButton,
            clearJWTTokenButton,
            simulateCrash,
            replaceEntropyButton,
            themeSelectionButton,
            resetTipsButton
        ]

        #if DEBUG
            rows.append(contentsOf: [openTrUAPIPlaygroundButton, pocketFacePreviewButton, pocketCardsButton])
        #endif

        rows.append(contentsOf: [strategyDebugRow, truApiRuntimeRow, hostPlacedRow])

        stackView.addArrangedSubviews(rows)

        stackView.arrangedSubviews.forEach { row in
            row.snp.makeConstraints { make in
                make.height.equalTo(44)
            }
        }
    }

    func setupButtonEnabled(_ button: RoundedButton, isEnabled: Bool) {
        button.isUserInteractionEnabled = isEnabled

        if isEnabled {
            button.imageWithTitleView?.titleColor = .fgPrimaryInverted
        } else {
            button.imageWithTitleView?.titleColor = .black30
        }
    }
}
