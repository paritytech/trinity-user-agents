import UIKit
import UIKit_iOS
import Foundation_iOS
import PolkadotUI
import DesignSystem

final class RecipientInputContentView: UIView {
    let titleLabel: Label = .create { label in
        label.typography = .titleMedium
        label.textColor = .fgSecondary
    }

    let accountPillView = AccountPillView()

    let textInputServiceView: TextWithServiceInputView = .create { (view: TextWithServiceInputView) in
        view.shouldUseClearButton = false
        view.apply(style: .addressInput)
        view.applyPasteButton(style: .addressInput)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupLayout() {
        addSubview(titleLabel)
        titleLabel.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(16)
            make.centerY.equalToSuperview()
        }

        titleLabel.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)
        titleLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)

        addSubview(accountPillView)
        accountPillView.snp.makeConstraints {
            $0.leading.equalTo(titleLabel.snp.trailing).offset(8)
            $0.centerY.equalToSuperview()
            $0.trailing.lessThanOrEqualToSuperview().inset(24)
            $0.height.equalTo(32)
            $0.top.bottom.equalToSuperview().priority(.high)
        }
        accountPillView.setContentHuggingPriority(.defaultHigh, for: .horizontal)

        addSubview(textInputServiceView)
        textInputServiceView.snp.makeConstraints { make in
            make.leading.equalTo(titleLabel.snp.trailing).offset(8)
            make.top.bottom.equalToSuperview()
            make.trailing.equalToSuperview().offset(-16)
        }
    }
}

final class RecipientInputView: RowView<RecipientInputContentView> {
    var titleLabel: UILabel {
        rowContentView.titleLabel
    }

    var textInputServiceView: TextWithServiceInputView {
        rowContentView.textInputServiceView
    }

    var textField: UITextField {
        textInputServiceView.textField
    }

    var resultLabel: UILabel {
        rowContentView.accountPillView.accountLabel
    }

    var inputValue: String? {
        rowContentView.textInputServiceView.inputViewModel?.inputHandler.value
    }

    convenience init() {
        self.init(frame: .zero)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        configure()
        setupHandlers()
    }

    func bind(inputViewModel: InputViewModelProtocol) {
        textInputServiceView.bind(inputViewModel: inputViewModel)

        textInputServiceView.textField.attributedPlaceholder = NSAttributedString(
            string: inputViewModel.placeholder,
            attributes: [
                .foregroundColor: UIColor.fgDisabled,
                .font: UIFont.bodyMedium
            ]
        )

        resultLabel.text = textField.text

        updateResultState()
    }

    func bind(accountType: SearchAccountViewModel.AccountType) {
        rowContentView.accountPillView.bind(account: accountType)
    }

    private func configure() {
        preferredHeight = 56

        contentInsets = .zero

        roundedBackgroundView.applyBackgroundStyle(.bgSurfaceMain, cornerRadius: 0)
        borderView.borderType = .bottom
        borderView.strokeColor = .appliedStroke
        borderView.strokeWidth = 1

        hasInteractableContent = true
    }

    private func setupHandlers() {
        textInputServiceView.addTarget(
            self,
            action: #selector(actionTextChanged),
            for: .editingChanged
        )

        textField.addTarget(
            self,
            action: #selector(actionEditingBeginEnd),
            for: .editingDidEnd
        )

        textField.addTarget(
            self,
            action: #selector(actionEditingBeginEnd),
            for: .editingDidBegin
        )

        let recognizer = BindableTapRecognizer { [weak self] in
            self?.textField.becomeFirstResponder()
        }
        recognizer.delegate = self
        rowContentView.addGestureRecognizer(recognizer)
    }

    private func updateResultState() {
        if !textField.isFirstResponder, let text = resultLabel.text, !text.isEmpty {
            rowContentView.accountPillView.isHidden = false
            textInputServiceView.isHidden = true
        } else {
            rowContentView.accountPillView.isHidden = true
            textInputServiceView.isHidden = false
        }
    }

    @objc func actionEditingBeginEnd() {
        updateResultState()
    }

    @objc func actionTextChanged() {
        resultLabel.text = textField.text
        updateResultState()
    }
}

// MARK: - UIGestureRecognizerDelegate

extension RecipientInputView: UIGestureRecognizerDelegate {
    func gestureRecognizer(_: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        if let view = touch.view, view.isDescendant(of: textInputServiceView) {
            return false
        }
        return true
    }
}

// MARK: private extensions

private extension TextWithServiceInputView.PasteButtonStyle {
    static var addressInput: TextWithServiceInputView.PasteButtonStyle {
        .init(
            roundedButtonStyle: .small,
            contentInsets: UIEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        )
    }
}

private extension TextInputView.Style {
    static var addressInput: TextInputView.Style {
        .init(
            fieldStyle: .init(
                font: UIFont.titleMedium,
                textColor: .fgPrimary,
                tintColor: .fgPrimary,
                textContentType: .nickname,
                smartQuotesType: .no,
                smartDashesType: .no,
                spellCheckingType: .no
            ),
            strokeOnEditing: .init(
                shadow: .init(
                    shadowOpacity: 0,
                    shadowColor: nil,
                    shadowRadius: nil,
                    shadowOffset: nil
                ),
                strokeWidth: 0,
                strokeColor: .clear,
                highlightedStrokeColor: .clear,
                fillColor: .clear,
                highlightedFillColor: .clear,
                rounding: .none
            ),
            clearButtonStyle: nil,
            contentInsets: UIEdgeInsets(top: 12, left: 0, bottom: 12, right: 0)
        )
    }
}

private extension RoundedButton.Style {
    static var small: RoundedButton.Style {
        .init(
            background: .init(
                shadow: .init(
                    shadowOpacity: 0,
                    shadowColor: nil,
                    shadowRadius: nil,
                    shadowOffset: nil
                ),
                fillColor: .bgActionSecondary,
                highlightedFillColor: .bgActionSecondaryHover,
                rounding: .init(
                    radius: 12,
                    corners: .allCorners
                )
            ),
            title: .init(
                normalColor: .fgPrimary,
                highlightedColor: .fgSecondary,
                font: UIFont.titleSmall
            )
        )
    }
}
