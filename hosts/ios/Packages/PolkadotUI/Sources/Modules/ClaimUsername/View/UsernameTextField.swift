import SwiftUI
import UIKit
import ExternalAccessibility
import Foundation_iOS

struct UsernameTextField: View {
    let inputViewModel: any InputViewModelProtocol
    @Binding var isFocused: Bool
    var onChange: (() -> Void)?
    @State private var hasText: Bool = false

    var body: some View {
        HStack(spacing: 8) {
            RawUsernameTextField(
                inputViewModel: inputViewModel,
                isFocused: $isFocused,
                hasText: $hasText,
                onChange: onChange
            )
            if hasText {
                Button {
                    inputViewModel.inputHandler.clearValue()
                    hasText = false
                    onChange?()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(Color(.fgTertiary))
                }
            }
        }
        .padding(.horizontal, 16)
        .frame(height: 48)
        .allowsHitTesting(true)
        .accessibilityId(AccessibilityID.Username.input)
    }
}

private struct RawUsernameTextField: UIViewRepresentable {
    let inputViewModel: any InputViewModelProtocol
    @Binding var isFocused: Bool
    @Binding var hasText: Bool
    var onChange: (() -> Void)?

    func makeUIView(context: Context) -> UITextField {
        let textField = UITextField()
        textField.delegate = context.coordinator
        textField.textContentType = .nickname
        textField.autocorrectionType = .no
        textField.spellCheckingType = .no
        textField.smartQuotesType = .no
        textField.smartDashesType = .no
        textField.smartInsertDeleteType = .no
        textField.autocapitalizationType = .none
        textField.borderStyle = .none
        textField.backgroundColor = .clear
        textField.textColor = .fgPrimary
        textField.font = .semibold16

        let placeholder = NSAttributedString(
            string: inputViewModel.placeholder,
            attributes: [.foregroundColor: UIColor.fgTertiary]
        )
        textField.attributedPlaceholder = placeholder

        textField.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        textField.addTarget(
            context.coordinator,
            action: #selector(Coordinator.editingChanged(_:)),
            for: .editingChanged
        )
        return textField
    }

    func updateUIView(_ textField: UITextField, context: Context) {
        if textField.text != inputViewModel.inputHandler.value {
            textField.text = inputViewModel.inputHandler.value
        }
        context.coordinator.inputViewModel = inputViewModel
        context.coordinator.onChange = onChange

        let newHasText = !inputViewModel.inputHandler.value.isEmpty
        if hasText != newHasText {
            DispatchQueue.main.async { hasText = newHasText }
        }

        let shouldFocus = isFocused
        DispatchQueue.main.async {
            if shouldFocus, !textField.isFirstResponder {
                textField.becomeFirstResponder()
            } else if !shouldFocus, textField.isFirstResponder {
                textField.resignFirstResponder()
            }
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(isFocused: $isFocused, hasText: $hasText, inputViewModel: inputViewModel, onChange: onChange)
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        @Binding var isFocused: Bool
        @Binding var hasText: Bool
        var inputViewModel: any InputViewModelProtocol
        var onChange: (() -> Void)?

        init(
            isFocused: Binding<Bool>,
            hasText: Binding<Bool>,
            inputViewModel: any InputViewModelProtocol,
            onChange: (() -> Void)?
        ) {
            _isFocused = isFocused
            _hasText = hasText
            self.inputViewModel = inputViewModel
            self.onChange = onChange
        }

        func textField(
            _ textField: UITextField,
            shouldChangeCharactersIn range: NSRange,
            replacementString string: String
        ) -> Bool {
            let shouldApply = inputViewModel.inputHandler.didReceiveReplacement(string, for: range)
            if !shouldApply, textField.text != inputViewModel.inputHandler.value {
                textField.text = inputViewModel.inputHandler.value
            }
            return shouldApply
        }

        @objc func editingChanged(_ textField: UITextField) {
            if textField.text != inputViewModel.inputHandler.value {
                textField.text = inputViewModel.inputHandler.value
            }
            hasText = !inputViewModel.inputHandler.value.isEmpty
            onChange?()
        }

        func textFieldDidBeginEditing(_: UITextField) {
            isFocused = true
        }

        func textFieldDidEndEditing(_: UITextField) {
            isFocused = false
        }

        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            textField.resignFirstResponder()
            return true
        }
    }
}
