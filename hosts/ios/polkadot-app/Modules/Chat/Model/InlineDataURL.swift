import Foundation

/// A `data:<mime type>;base64,<payload>` URL. Products send small images in this form.
struct InlineDataURL: Equatable {
    let mimeType: String
    let data: Data

    init?(_ string: String) {
        guard string.hasPrefix("data:"), let marker = string.range(of: ";base64,") else {
            return nil
        }

        let mimeStart = string.index(string.startIndex, offsetBy: "data:".count)
        guard let data = Data(base64Encoded: String(string[marker.upperBound...])) else {
            return nil
        }

        mimeType = String(string[mimeStart ..< marker.lowerBound]).lowercased()
        self.data = data
    }
}
