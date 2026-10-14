import Foundation
import Foundation_iOS

extension InputViewModel {
    static func createAccountInputViewModel(
        for value: String,
        title: String = "",
        placeholder: String = String(localized: .recipientInputPlaceholder),
        required: Bool = true
    ) -> InputViewModelProtocol {
        let inputHandler = InputHandler(
            value: value,
            required: required,
            predicate: required ? NSPredicate.notEmpty : nil,
            processor: TrimmingCharacterProcessor(charset: CharacterSet.whitespacesAndNewlines)
        )

        let viewModel = InputViewModel(
            inputHandler: inputHandler,
            title: title,
            placeholder: placeholder
        )
        return viewModel
    }

    static func createUsernameInputViewModel(for metadata: UsernameMetadata) -> InputViewModelProtocol {
        let inputHandler = InputHandler(
            value: "",
            required: true,
            maxLength: metadata.maxLength,
            predicate: .getUsername(for: metadata.minLength, maxLength: metadata.maxLength),
            processor: UppercasePreprocessor(charset: .whitespacesAndNewlines),
            normalizer: LowercasePreprocessor(charset: .whitespacesAndNewlines)
        )

        let viewModel = InputViewModel(inputHandler: inputHandler, title: "")
        return viewModel
    }

    static func createMnemonicInputViewModel() -> InputViewModelProtocol {
        let inputHandler = InputHandler(
            value: "",
            required: true,
            maxLength: 250,
            validCharacterSet: .englishMnemonic,
            predicate: NSPredicate.notEmpty,
            normalizer: MnemonicTextNormalizer()
        )

        let viewModel = InputViewModel(inputHandler: inputHandler, title: "")
        return viewModel
    }
}
