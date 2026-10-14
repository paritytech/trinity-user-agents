import Foundation
import Foundation_iOS

struct UppercasePreprocessor: TextProcessing {
    let charset: CharacterSet

    func process(text: String) -> String {
        text.trimmingCharacters(in: charset).uppercased()
    }
}
