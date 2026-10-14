import Foundation
import Products

struct DetectedProductLink {
    let host: ProductHost
    let matchedURL: String
}

enum ProductURLDetector {
    static func firstProductLink(
        in text: String,
        hostProvider: ProductHostProviding
    ) -> DetectedProductLink? {
        guard !text.isEmpty else { return nil }

        guard let detector = try? NSDataDetector(
            types: NSTextCheckingResult.CheckingType.link.rawValue
        ) else {
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
