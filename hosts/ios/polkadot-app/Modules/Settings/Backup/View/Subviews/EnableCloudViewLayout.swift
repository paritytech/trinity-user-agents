import UIKit
import UIKit_iOS
import SnapKit
import PolkadotUI
import DesignSystem

final class EnableCloudViewLayout: BottomSheetBaseLayout {
    private let titleLabel: Label = .create {
        $0.typography = .headlineSmall
        $0.textColor = .fgPrimary
        $0.text = String(localized: .cloudBackupNotAvailableTitle)
    }

    private let subtitleLabel: Label = .create {
        $0.typography = .paragraphLarge
        $0.textColor = .fgSecondary
        $0.numberOfLines = 0
        $0.text = String(localized: .cloudBackupNotAvailableDescription)
    }

    private let stepsContainerView: UIView = .create {
        $0.backgroundColor = .bgSurfaceNested
        $0.layer.cornerRadius = 24
    }

    let openSettingsButton: RoundedButton = .create {
        $0.apply(style: .white)
        $0.setTitle(String(localized: .cloudBackupNotAvailableActionSettings))
        $0.changesContentOpacityWhenHighlighted = true
    }

    override func setupLayout() {
        super.setupLayout()

        let titleStack = UIStackView(arrangedSubviews: [titleLabel, subtitleLabel])
        titleStack.axis = .vertical
        titleStack.spacing = 8
        titleStack.isLayoutMarginsRelativeArrangement = true
        titleStack.layoutMargins = UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)

        let stepsStack = makeStepsStack()
        stepsContainerView.addSubview(stepsStack)
        stepsStack.snp.makeConstraints {
            $0.edges.equalToSuperview().inset(24)
        }

        let mainStack = UIStackView(arrangedSubviews: [titleStack, stepsContainerView])
        mainStack.axis = .vertical
        mainStack.spacing = 24

        contentView.addSubview(mainStack)
        contentView.addSubview(openSettingsButton)

        mainStack.snp.makeConstraints {
            $0.top.leading.trailing.equalToSuperview()
        }

        openSettingsButton.snp.makeConstraints {
            $0.leading.trailing.bottom.equalToSuperview()
            $0.top.equalTo(mainStack.snp.bottom).offset(32)
            $0.height.equalTo(UIConstants.actionHeight)
        }
    }
}

// MARK: - Private functions

extension EnableCloudViewLayout {
    private func makeStepsStack() -> UIStackView {
        let steps = [
            String(localized: .cloudBackupNotAvailableStep1),
            String(localized: .cloudBackupNotAvailableStep2),
            String(localized: .cloudBackupNotAvailableStep3)
        ]

        let stepViews = steps.enumerated().map { index, text in
            makeStepRow(number: index + 1, text: text)
        }

        let stack = UIStackView(arrangedSubviews: stepViews)
        stack.axis = .vertical
        stack.spacing = 16
        return stack
    }

    private func makeStepRow(number: Int, text: String) -> UIView {
        let numberLabel = Label()
        numberLabel.text = "\(number)"
        numberLabel.typography = .titleMedium
        numberLabel.textColor = .fgPrimary
        numberLabel.textAlignment = .center

        let numberBackground = UIView()
        numberBackground.backgroundColor = .bgActionTertiary
        numberBackground.layer.cornerRadius = 16
        numberBackground.addSubview(numberLabel)

        numberLabel.snp.makeConstraints {
            $0.center.equalToSuperview()
        }
        numberBackground.snp.makeConstraints {
            $0.size.equalTo(32)
        }

        let textLabel = Label()
        textLabel.text = text
        textLabel.typography = .titleMedium
        textLabel.textColor = .fgPrimary
        textLabel.numberOfLines = 0

        let row = UIStackView(arrangedSubviews: [numberBackground, textLabel])
        row.axis = .horizontal
        row.spacing = 12
        row.alignment = .center

        return row
    }
}
