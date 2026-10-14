import Foundation
import Foundation_iOS

extension InputViewModel {
    static func createUsernameInputViewModel(
        for username: Username?,
        metadata: UsernameMetadata
    ) -> InputViewModelProtocol {
        let inputHandler = InputHandler(
            value: username?.partialUsername ?? "",
            required: true,
            maxLength: metadata.maxLength,
            validCharacterSet: .username,
            predicate: .getUsername(for: metadata.minLength, maxLength: metadata.maxLength),
            processor: TrimmingCharacterProcessor(charset: CharacterSet.whitespacesAndNewlines)
        )

        let viewModel = InputViewModel(
            inputHandler: inputHandler,
            title: "",
            placeholder: String(localized: .claimUsernamePlaceholder)
        )
        return viewModel
    }

    static func createDigitsInputViewModel(
        initialValue: String = ""
    ) -> InputViewModelProtocol {
        let inputHandler = InputHandler(
            value: initialValue,
            required: true,
            maxLength: 2,
            validCharacterSet: .decimalDigits,
            predicate: NSPredicate(format: "SELF MATCHES %@", "[0-9]{2}")
        )

        return InputViewModel(inputHandler: inputHandler, title: "")
    }
}
