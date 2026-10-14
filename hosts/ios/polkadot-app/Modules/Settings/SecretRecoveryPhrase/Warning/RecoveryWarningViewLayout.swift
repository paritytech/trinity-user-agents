import UIKit
import UIKit_iOS
import ExternalAccessibility
import PolkadotUI
import DesignSystem

final class RecoveryWarningViewLayout: BottomSheetBaseLayout {
    struct Model {
        let icon: UIImage
        let text: String
    }

    let layoutContent: RecoveryWarningViewContentView = .create {
        $0.actionsStackView.axis = .vertical
        $0.actionsStackView.spacing = DSButtonStyle.Size.mediumIncreased.verticalPadding
    }

    let actionButton = DSButtonView("", size: .mediumIncreased, expands: true)

    let closeButton = DSButtonView("", style: .tertiary, size: .mediumIncreased, expands: true)

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func setupLayout() {
        super.setupLayout()
        contentView.addSubview(layoutContent)
        layoutContent.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        layoutContent.actionsStackView.addArrangedSubview(actionButton)
        layoutContent.actionsStackView.addArrangedSubview(closeButton)
    }
}

final class RecoveryWarningViewContentView: UIView {
    let actionsStackView: UIStackView = .create { view in
        view.axis = .vertical
        view.distribution = .fillEqually
    }

    private let stepsStackView: UIStackView = .create {
        $0.axis = .vertical
        $0.spacing = 16
    }

    var steps: [RecoveryWarningViewLayout.Model] = [] {
        didSet {
            reloadSteps()
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setupLayout() {
        addSubview(actionsStackView)
        actionsStackView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.bottom.equalToSuperview()
        }

        addSubview(stepsStackView)
        stepsStackView.snp.makeConstraints { make in
            make.top.leading.trailing.equalToSuperview()
            make.bottom.equalTo(actionsStackView.snp.top).offset(-32)
        }
    }
}

private extension RecoveryWarningViewContentView {
    func reloadSteps() {
        stepsStackView.subviews.forEach { $0.removeFromSuperview() }
        let views = steps.map {
            let row = GenericPairValueView<UIImageView, UILabel>()
            row.makeHorizontal()
            row.spacing = 16
            row.stackView.alignment = .center
            row.stackView.distribution = .fillProportionally

            row.fView.image = $0.icon.withRenderingMode(.alwaysTemplate)
            row.fView.tintColor = .fgPrimary
            row.fView.snp.makeConstraints { make in
                make.height.equalTo(row.fView.snp.width)
                make.height.equalTo(24)
            }

            row.sView.text = $0.text
            row.sView.numberOfLines = 3
            row.sView.textColor = .fgPrimary
            row.sView.font = .regular16
            return row
        }
        stepsStackView.addArrangedSubviews(views)
    }
}

// MARK: - AccessibilityBound

extension RecoveryWarningViewLayout: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [.init(actionButton, AccessibilityID.Settings.showRecoveryPhraseButton)]
    }
}
