import Foundation
import Foundation_iOS

struct LowercasePreprocessor: TextProcessing {
    let charset: CharacterSet

    func process(text: String) -> String {
        text.trimmingCharacters(in: charset).lowercased()
    }
}
