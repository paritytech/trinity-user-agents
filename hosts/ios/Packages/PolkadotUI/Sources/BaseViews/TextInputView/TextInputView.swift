import UIKit
public import UIKit_iOS
import Foundation_iOS

public class TextInputView: BackgroundedContentControl {
    let textFieldContainer = UIView()

    public let textField: UITextField = {
        let view = UITextField()
        view.clearButtonMode = .never

        var attributes = view.defaultTextAttributes
        let currentStyle = attributes[.paragraphStyle] as? NSParagraphStyle
        let paragraphStyle = (currentStyle?.mutableCopy() as? NSMutableParagraphStyle) ??
            NSMutableParagraphStyle()
        paragraphStyle.lineBreakMode = .byTruncatingTail

        attributes[.paragraphStyle] = paragraphStyle
        view.defaultTextAttributes = attributes

        view.keyboardType = .default
        view.returnKeyType = .done
        view.autocorrectionType = .no
        view.autocapitalizationType = .none
        view.borderStyle = .none

        return view
    }()

    let clearButton: RoundedButton = {
        let button = RoundedButton()
        button.imageWithTitleView?.spacingBetweenLabelAndIcon = 0
        return button
    }()

    public var shouldUseClearButton: Bool = true {
        didSet {
            applyControlsState()
        }
    }

    let stackView: UIStackView = {
        let view = UIStackView()
        view.spacing = 8.0
        view.axis = .horizontal
        view.alignment = .fill
        return view
    }()

    public weak var delegate: TextInputViewDelegate?

    public var roundedBackgroundView: RoundedView? {
        backgroundView as? RoundedView
    }

    public private(set) var inputViewModel: InputViewModelProtocol?

    public var completed: Bool {
        if let inputViewModel {
            inputViewModel.inputHandler.completed
        } else {
            false
        }
    }

    public var hasText: Bool {
        if let text = textField.text, !text.isEmpty {
            true
        } else {
            false
        }
    }

    override public var accessibilityIdentifier: String? {
        get { textField.accessibilityIdentifier }
        set { textField.accessibilityIdentifier = newValue }
    }

    override public var intrinsicContentSize: CGSize {
        CGSize(width: UIView.noIntrinsicMetric, height: 48.0)
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        configure()
    }

    public func bind(inputViewModel: InputViewModelProtocol) {
        if textField.text != inputViewModel.inputHandler.value {
            textField.text = inputViewModel.inputHandler.value
        }

        if !inputViewModel.placeholder.isEmpty {
            textField.placeholder = inputViewModel.placeholder
        }

        textField.isEnabled = inputViewModel.inputHandler.enabled

        self.inputViewModel = inputViewModel

        updateControlsState()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Layout

    override public func layoutSubviews() {
        super.layoutSubviews()

        contentView?.frame = bounds

        layoutContent()
    }

    func applyingActionWidth(for currentWidth: CGFloat) -> CGFloat {
        if !clearButton.isHidden {
            currentWidth + clearButton.intrinsicContentSize.width
        } else {
            currentWidth
        }
    }

    private func layoutContent() {
        let buttonHeight: CGFloat = 32.0
        let actionsWidth: CGFloat = applyingActionWidth(for: 0)

        stackView.frame = CGRect(
            x: bounds.maxX - contentInsets.right - actionsWidth,
            y: bounds.midY - buttonHeight / 2.0,
            width: actionsWidth,
            height: buttonHeight
        )

        let rightFieldSpacing: CGFloat = 8.0
        let fieldWidth: CGFloat =
            if actionsWidth > 0 {
                max(
                    stackView.frame.minX - contentInsets.left - rightFieldSpacing,
                    0
                )
            } else {
                max(stackView.frame.minX - contentInsets.left, 0)
            }

        let fieldHeight = textField.intrinsicContentSize.height
        textFieldContainer.frame = CGRect(
            x: contentInsets.left,
            y: bounds.midY - fieldHeight / 2.0,
            width: fieldWidth,
            height: fieldHeight
        )

        textField.frame = textFieldContainer.bounds
    }

    // MARK: Configure

    func configure() {
        backgroundColor = UIColor.clear

        configureBackgroundViewIfNeeded()
        configureContentViewIfNeeded()
        configureLocalHandlers()
        configureTextFieldHandlers()
        configureClearHandlers()

        updateControlsState()
    }

    private func configureBackgroundViewIfNeeded() {
        if backgroundView == nil {
            let roundedView = RoundedView()
            roundedView.isUserInteractionEnabled = false

            backgroundView = roundedView
        }
    }

    func configureContentViewIfNeeded() {
        if contentView == nil {
            let contentView = UIView()
            contentView.backgroundColor = .clear
            contentView.isUserInteractionEnabled = false
            self.contentView = contentView
        }

        addSubview(textFieldContainer)
        textFieldContainer.addSubview(textField)

        stackView.addArrangedSubview(clearButton)

        addSubview(stackView)
    }

    private func configureTextFieldHandlers() {
        textField.addTarget(self, action: #selector(actionEditingBeginEnd), for: .editingDidBegin)
        textField.addTarget(self, action: #selector(actionEditingBeginEnd), for: .editingDidEnd)
        textField.addTarget(
            self,
            action: #selector(actionEditingChanged(_:)),
            for: .editingChanged
        )

        textField.delegate = self
    }

    private func configureClearHandlers() {
        clearButton.addTarget(
            self,
            action: #selector(actionClear),
            for: .touchUpInside
        )
    }

    private func configureLocalHandlers() {
        addTarget(self, action: #selector(actionTouchUpInside), for: .touchUpInside)
    }

    func applyControlsState() {
        if shouldUseClearButton, hasText, textField.isEnabled {
            clearButton.isHidden = false
        } else {
            clearButton.isHidden = true
        }
    }

    func updateControlsState() {
        let oldStates = stackView.arrangedSubviews.map(\.isHidden)

        applyControlsState()

        let newStates = stackView.arrangedSubviews.map(\.isHidden)

        if oldStates != newStates {
            setNeedsLayout()
        }
    }

    // MARK: Action

    @objc private func actionEditingChanged(_ sender: UITextField) {
        if inputViewModel?.inputHandler.value != sender.text {
            sender.text = inputViewModel?.inputHandler.value
        }

        updateControlsState()

        sendActions(for: .editingChanged)
    }

    @objc private func actionEditingBeginEnd() {
        roundedBackgroundView?.strokeWidth = textField.isFirstResponder ? 0.5 : 0

        updateControlsState()
    }

    @objc private func actionTouchUpInside() {
        textField.becomeFirstResponder()
    }

    @objc func actionClear() {
        guard hasText else {
            return
        }

        textField.text = ""
        inputViewModel?.inputHandler.changeValue(to: "")

        updateControlsState()

        sendActions(for: .editingChanged)
    }
}

extension TextInputView: UITextFieldDelegate {
    public func textField(
        _ textField: UITextField,
        shouldChangeCharactersIn range: NSRange,
        replacementString string: String
    ) -> Bool {
        guard let inputViewModel else {
            return true
        }

        let shouldApply = inputViewModel.inputHandler.didReceiveReplacement(string, for: range)

        if !shouldApply, textField.text != inputViewModel.inputHandler.value {
            textField.text = inputViewModel.inputHandler.value
        }

        return shouldApply
    }

    public func textFieldShouldClear(_: UITextField) -> Bool {
        inputViewModel?.inputHandler.changeValue(to: "")

        return true
    }

    public func textFieldShouldReturn(_: UITextField) -> Bool {
        if let delegate {
            return delegate.textInputViewShouldReturn(self)
        } else {
            textField.resignFirstResponder()
            return true
        }
    }

    public func textFieldShouldBeginEditing(_: UITextField) -> Bool {
        delegate?.textInputViewWillStartEditing(self)
        return true
    }
}
