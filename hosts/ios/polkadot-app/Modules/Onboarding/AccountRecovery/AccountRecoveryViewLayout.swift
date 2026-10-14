import Foundation
import Foundation_iOS
import UIKit
import UIKit_iOS
import ExternalAccessibility
import PolkadotUI
import AudioToolbox
import FoundationExt
import DesignSystem

class AccountRecoveryViewLayout: ScrollableContainerLayoutView {
    let proceedButton: RoundedButton = .create { view in
        view.applyMainStyle()
        view.setTitle(String(localized: .accountRecoveryAction))
    }

    var mnemonicText: String? {
        textView.text
    }

    let textView: RoundedTextView = create {
        $0.placeholder = String(localized: .accountRecoveryInputPlaceholder)
    }

    private let topLabelsView: TopBottomLabelView = .create { view in
        view.topLabel.typography = .titleLarge
        view.topLabel.textColor = .fgPrimary
        view.topLabel.numberOfLines = 0
        view.topLabel.text = String(localized: .accountRecoveryTitle)

        view.bottomLabel.typography = .bodyLarge
        view.bottomLabel.textColor = .fgTertiary
        view.bottomLabel.numberOfLines = 0
        view.bottomLabel.text = String(localized: .accountRecoveryDescription)

        view.spacing = 8
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        applyAccessibilityBindings()
        addHandlers()
    }

    override func setupLayout() {
        super.setupLayout()

        layoutInsets = .init(
            top: 8,
            left: 24,
            bottom: 0,
            right: 24
        )

        addArrangedSubview(topLabelsView, spacingAfter: 24)

        textView.snp.makeConstraints {
            $0.height.equalTo(136)
        }
        addArrangedSubview(textView, spacingAfter: 24)

        addSubview(proceedButton)
        proceedButton.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(UIConstants.horizontalInsetWide)
            make.bottom.equalTo(safeAreaLayoutGuide).offset(-UIConstants.verticalInsetWide)
            make.height.equalTo(UIConstants.actionHeight)
        }
    }

    override func setupStyle() {
        super.setupStyle()

        backgroundColor = .bgSurfaceMain
        containerView.scrollView.showsVerticalScrollIndicator = false
    }
}

// MARK: - ViewModel

extension AccountRecoveryViewLayout {
    func bind(inputViewModel: InputViewModelProtocol) {
        textView.bind(inputViewModel: inputViewModel)

        setupProceedButton()
    }
}

// MARK: - KeyboardAdoptableViewLayout

extension AccountRecoveryViewLayout: KeyboardAdoptableViewLayout {
    func adoptToVisibleKeyboard(bottomInset: CGFloat) {
        proceedButton.snp.remakeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(UIConstants.horizontalInsetWide)
            make.bottom.equalToSuperview().inset(bottomInset + UIConstants.verticalInsetWide)
            make.height.equalTo(UIConstants.actionHeight)
        }
    }

    func adoptToHiddenKeyboard() {
        proceedButton.snp.remakeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(UIConstants.horizontalInsetWide)
            make.bottom.equalTo(safeAreaLayoutGuide).offset(-UIConstants.verticalInsetWide)
            make.height.equalTo(UIConstants.actionHeight)
        }
    }
}

// MARK: - Private

private extension AccountRecoveryViewLayout {
    func addHandlers() {
        textView.addTarget(
            self,
            action: #selector(actionTextChanged),
            for: .editingChanged
        )
    }

    @objc
    func actionTextChanged() {
        setupProceedButton()
    }

    func setupProceedButton() {
        if let handler = textView.inputViewModel?.inputHandler, handler.completed {
            proceedButton.isUserInteractionEnabled = true
            proceedButton.applyMainStyle()
        } else {
            proceedButton.isUserInteractionEnabled = false
            proceedButton.applyDisabledStyle()
        }
    }
}

// MARK: - AccessibilityBound

extension AccountRecoveryViewLayout: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [
            .init(textView, AccessibilityID.RecoveryPhrase.input),
            .init(proceedButton, AccessibilityID.RecoveryPhrase.submitButton)
        ]
    }
}
