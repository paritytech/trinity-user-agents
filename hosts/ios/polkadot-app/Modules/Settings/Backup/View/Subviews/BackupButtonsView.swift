import UIKit
import UIKit_iOS
import SnapKit
import ExternalAccessibility
import PolkadotUI
import DesignSystem

protocol BackupButtonsViewDelegate: AnyObject {
    func didTapButton(_ type: BackupButtonsView.ButtonType)
}

final class BackupButtonsView: UIView {
    // MARK: Properties

    enum ButtonType {
        case backup
        case settings
        case secretPhase
    }

    weak var delegate: BackupButtonsViewDelegate?
    private var buttonsView = UIStackView()
    private let loadingView: LoadingView = .create {
        $0.contentBackgroundColor = .bgSurfaceContainer
        $0.contentCornerRadius = Constants.cornerRadiusLoadingButton
        $0.indicatorImage = .loadingIndicator.withRenderingMode(.alwaysTemplate)
        $0.tintColor = .fgPrimary
    }

    private let backupAccount = DSButtonView(
        String(localized: .backupActionBackup),
        expands: true
    )

    private let settingsButton = DSButtonView(
        String(localized: .backupActionSettings),
        expands: true
    )

    private let viewSecretPhaseButton = DSButtonView(
        String(localized: .backupActionViewSecret),
        style: .secondary,
        expands: true
    )

    override var intrinsicContentSize: CGSize {
        let buttonHeight = viewSecretPhaseButton.proposedHeight
        let height =
            if buttonsView.arrangedSubviews.count == 1 {
                buttonHeight
            } else {
                Constants.spacingBetweenButtons + CGFloat(buttonsView.arrangedSubviews.count) * buttonHeight
            }

        return CGSize(
            width: UIView.noIntrinsicMetric,
            height: height
        )
    }

    // MARK: Initial methods

    override init(frame: CGRect) {
        super.init(frame: frame)

        configureView()
        configureActions()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Public methods

    func bind(model: BackupViewModel.BackupStatusType) {
        buttonsView.arrangedSubviews.forEach {
            $0.removeFromSuperview()
        }

        defer {
            buttonsView.addArrangedSubview(viewSecretPhaseButton)
            invalidateIntrinsicContentSize()
        }
        switch model {
        case .created:
            break
        case .cloudIsOff:
            buttonsView.addArrangedSubview(settingsButton)
        case .notFound:
            buttonsView.addArrangedSubview(backupAccount)
        }
    }

    // MARK: Private methods

    private func configureView() {
        buttonsView = .vStack(
            alignment: .fill,
            distribution: .fillEqually,
            spacing: Constants.spacingBetweenButtons,
            [viewSecretPhaseButton]
        )
        addSubview(buttonsView)

        buttonsView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }

    private func configureActions() {
        backupAccount.onTap = { [weak self] in
            guard let self else { return }
            showLoadingButton()
            delegate?.didTapButton(.backup)
        }

        settingsButton.onTap = { [weak self] in
            self?.delegate?.didTapButton(.settings)
        }

        viewSecretPhaseButton.onTap = { [weak self] in
            self?.delegate?.didTapButton(.secretPhase)
        }
    }

    private func showLoadingButton() {
        buttonsView.removeArrangedSubview(backupAccount)
        backupAccount.removeFromSuperview()
        loadingView.contentSize = CGSize(width: buttonsView.frame.width, height: backupAccount.proposedHeight)
        buttonsView.insertArrangedSubview(loadingView, at: .zero)
        loadingView.startAnimating()
    }
}

// MARK: - Constants

private enum Constants {
    static let spacingBetweenButtons: CGFloat = 16
    static let cornerRadiusLoadingButton: CGFloat = 24
}

// MARK: - AccessibilityBound

extension BackupButtonsView: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [
            .init(backupAccount, AccessibilityID.Backup.toIcloudButton),
            .init(viewSecretPhaseButton, AccessibilityID.Settings.viewRecoveryPhraseButton)
        ]
    }
}
