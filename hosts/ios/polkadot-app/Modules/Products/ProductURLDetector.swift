import Foundation
import Products

struct DetectedProductLink {
    let host: ProductHost
    let matchedURL: String
}

enum ProductURLDetector {
    /// One detector for the app. A detector is immutable and safe on any thread, and it is slow to make, and the
    /// chat screen asks for each text message at each list update (P7.1, B3).
    private static let linkDetector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)

    static func firstProductLink(
        in text: String,
        hostProvider: ProductHostProviding
    ) -> DetectedProductLink? {
        guard !text.isEmpty else { return nil }

        guard let detector = linkDetector else {
            return nil
        }

        let range = NSRange(text.startIndex ..< text.endIndex, in: text)
        let matches = detector.matches(in: text, options: [], range: range)

        for match in matches {
            guard
                let url = match.url,
                let productHost = hostProvider.host(url: url),
                let matchedRange = Range(match.range, in: text)
            else {
                continue
            }

            let matchedURL = String(text[matchedRange])
            return DetectedProductLink(host: productHost, matchedURL: matchedURL)
        }

        return nil
    }
}
