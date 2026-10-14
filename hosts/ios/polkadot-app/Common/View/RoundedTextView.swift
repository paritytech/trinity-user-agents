import UIKit
import UIKit_iOS
import Foundation_iOS
import DesignSystem
import PolkadotUI

final class RoundedTextView: UIControl {
    var textInsets = NSDirectionalEdgeInsets(
        top: 16,
        leading: 16,
        bottom: 16,
        trailing: 16
    ) {
        didSet {
            setupTextInsets()
        }
    }

    var placeholder: String? {
        get { placeholderLabel.text }
        set { placeholderLabel.text = newValue }
    }

    var text: String? {
        get { textView.text }
        set { textView.text = newValue }
    }

    var completed: Bool {
        if let inputViewModel {
            inputViewModel.inputHandler.completed
        } else {
            false
        }
    }

    override var accessibilityIdentifier: String? {
        get { textView.accessibilityIdentifier }
        set { textView.accessibilityIdentifier = newValue }
    }

    private let backgroundView: RoundedView = create {
        $0.applyBorderStyle(.white12, backgroundColor: .bgActionSecondary, cornerRadius: 12)
    }

    private(set) var inputViewModel: InputViewModelProtocol?

    private let textView: UITextView = create {
        $0.font = UIFont.paragraphLarge
        $0.textColor = .fgPrimary
        $0.tintColor = .fgPrimary
        $0.backgroundColor = .clear
        $0.textContainerInset = .zero
        $0.textContainer.lineFragmentPadding = 0
        $0.autocapitalizationType = .none
        $0.showsVerticalScrollIndicator = false
        $0.showsHorizontalScrollIndicator = false
    }

    private let placeholderLabel: Label = create {
        $0.typography = .paragraphLarge
        $0.textColor = .fgTertiary
        $0.numberOfLines = 0
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
        setupPlaceholder()
        textView.delegate = self
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @discardableResult
    override func becomeFirstResponder() -> Bool {
        textView.becomeFirstResponder()
    }

    @discardableResult
    override func resignFirstResponder() -> Bool {
        textView.resignFirstResponder()
    }

    func bind(inputViewModel: InputViewModelProtocol) {
        if textView.text != inputViewModel.inputHandler.value {
            textView.text = inputViewModel.inputHandler.value
        }

        if !inputViewModel.placeholder.isEmpty {
            placeholder = inputViewModel.placeholder
        }

        setupPlaceholder()

        self.inputViewModel = inputViewModel
    }
}

// MARK: - Private

private extension RoundedTextView {
    func setupLayout() {
        addSubview(backgroundView)
        backgroundView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        addSubview(textView)
        setupTextInsets()

        addSubview(placeholderLabel)
        placeholderLabel.snp.makeConstraints {
            $0.top.leading.trailing.equalToSuperview().inset(16)
        }
    }

    func setupPlaceholder() {
        placeholderLabel.isHidden = !textView.text.isEmpty
    }

    func setupTextInsets() {
        textView.snp.remakeConstraints {
            $0.top.equalToSuperview().inset(textInsets.top)
            $0.bottom.equalToSuperview().inset(textInsets.bottom)
            $0.leading.equalToSuperview().inset(textInsets.leading)
            $0.trailing.equalToSuperview().inset(textInsets.trailing)
        }
    }
}

// MARK: - UITextViewDelegate

extension RoundedTextView: UITextViewDelegate {
    func textViewDidChange(_ textView: UITextView) {
        if inputViewModel?.inputHandler.value != textView.text {
            textView.text = inputViewModel?.inputHandler.value
        }

        setupPlaceholder()

        sendActions(for: .editingChanged)
    }

    func textView(
        _ textView: UITextView,
        shouldChangeTextIn range: NSRange,
        replacementText text: String
    ) -> Bool {
        guard let inputViewModel else {
            return true
        }

        let shouldApply = inputViewModel.inputHandler.didReceiveReplacement(text, for: range)

        if !shouldApply, textView.text != inputViewModel.inputHandler.value {
            textView.text = inputViewModel.inputHandler.value
        }

        return shouldApply
    }
}
