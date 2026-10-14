import DesignSystem
import UIKit
public import UIKit_iOS

public final class TextWithServiceInputView: TextInputView {
    private enum Metrics {
        static let fieldCornerRadius: CGFloat = DSRadii.large
    }

    private let containerView = DSChatInputGlassBackground(
        cornerRadius: Metrics.fieldCornerRadius,
        fallbackColor: .bgActionTertiary,
        interactive: false
    )

    let pasteboardService = PasteboardHandler(pasteboard: UIPasteboard.general)

    public let pasteButton = RoundedButton()

    override func configure() {
        super.configure()

        textFieldContainer.backgroundColor = .clear
        configurePasteHandlers()
    }

    override public func layoutSubviews() {
        super.layoutSubviews()

        let height: CGFloat = 36
        let fieldWidth = textFieldContainer.frame.width

        textFieldContainer.frame = CGRect(
            x: contentInsets.left,
            y: bounds.midY - height / 2.0,
            width: fieldWidth,
            height: height
        )

        containerView.frame = textFieldContainer.bounds
        textField.frame = textFieldContainer.bounds.insetBy(dx: 12, dy: 0)

        let buttonHeight = pasteButton.intrinsicContentSize.height
        stackView.frame = stackView.frame.insetBy(dx: 0, dy: min(buttonHeight, height) - height)
    }

    override func configureContentViewIfNeeded() {
        super.configureContentViewIfNeeded()

        textFieldContainer.insertSubview(containerView, at: 0)
        stackView.addArrangedSubview(pasteButton)
    }

    private func configurePasteHandlers() {
        pasteButton.addTarget(self, action: #selector(actionPaste), for: .touchUpInside)

        pasteboardService.delegate = self
    }

    override func applyingActionWidth(for currentWidth: CGFloat) -> CGFloat {
        let actionWidth = super.applyingActionWidth(for: currentWidth)

        if !pasteButton.isHidden {
            return actionWidth + pasteButton.intrinsicContentSize.width
        } else {
            return actionWidth
        }
    }

    override func applyControlsState() {
        if hasText, textField.isEnabled {
            clearButton.isHidden = !shouldUseClearButton
            pasteButton.isHidden = true
        } else if !textField.isEnabled {
            clearButton.isHidden = true
            pasteButton.isHidden = true
        } else {
            clearButton.isHidden = true
            pasteButton.isHidden = !pasteboardService.pasteboard.hasStrings
        }
    }

    @objc func actionPaste() {
        if
            let pasteString = pasteboardService.pasteboard.string,
            let inputViewModel,
            inputViewModel.inputHandler.value != pasteString {
            let currentValue = inputViewModel.inputHandler.value
            let currentLength = (currentValue as NSString).length
            let range = NSRange(location: 0, length: currentLength)

            _ = inputViewModel.inputHandler.didReceiveReplacement(pasteString, for: range)

            if currentValue != inputViewModel.inputHandler.value {
                textField.text = inputViewModel.inputHandler.value
                sendActions(for: .editingChanged)
            }

            updateControlsState()
        }
    }
}

extension TextWithServiceInputView: PasteboardHandlerDelegate {
    func didReceivePasteboardChange(notification _: Notification) {
        updateControlsState()
    }

    func didReceivePasteboardRemove(notification _: Notification) {
        updateControlsState()
    }
}
