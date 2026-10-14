import Foundation

protocol ProductLinkHTMLParsing: Sendable {
    func title(in html: String) -> String?
    func iconRelativePath(in html: String) -> String?
}

final class ProductLinkHTMLParser: ProductLinkHTMLParsing {
    func title(in html: String) -> String? {
        firstCapture(in: html, pattern: #"<title[^>]*>([^<]*)</title>"#)
            .flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .flatMap { $0.isEmpty ? nil : $0 }
    }

    func iconRelativePath(in html: String) -> String? {
        if let path = firstCapture(in: html, pattern: relIconFirstPattern) {
            return path
        }
        return firstCapture(in: html, pattern: hrefFirstPattern)
    }
}

private extension ProductLinkHTMLParser {
    var relIconFirstPattern: String {
        #"<link[^>]+rel=["'](?:shortcut )?icon["'][^>]*href=["']([^"']+)["']"#
    }

    var hrefFirstPattern: String {
        #"<link[^>]+href=["']([^"']+)["'][^>]*rel=["'](?:shortcut )?icon["']"#
    }

    func firstCapture(in html: String, pattern: String) -> String? {
        let fullRange = NSRange(html.startIndex ..< html.endIndex, in: html)
        guard
            let regex = try? NSRegularExpression(pattern: pattern, options: .caseInsensitive),
            let match = regex.firstMatch(in: html, range: fullRange),
            match.numberOfRanges > 1,
            let range = Range(match.range(at: 1), in: html)
        else {
            return nil
        }
        return String(html[range])
    }
}
