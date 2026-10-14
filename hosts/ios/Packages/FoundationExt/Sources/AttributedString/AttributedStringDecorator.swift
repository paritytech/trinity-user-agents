import UIKit

public protocol AttributedStringDecoratorProtocol: AnyObject {
    func decorate(attributedString: NSAttributedString) -> NSAttributedString
}

public final class HighlightingAttributedStringDecorator: AttributedStringDecoratorProtocol {
    let pattern: String
    let attributes: [NSAttributedString.Key: Any]
    let includeSeparator: Bool

    public init(pattern: String, attributes: [NSAttributedString.Key: Any], includeSeparator: Bool = false) {
        self.pattern = pattern
        self.attributes = attributes
        self.includeSeparator = includeSeparator
    }

    public func decorate(attributedString: NSAttributedString) -> NSAttributedString {
        let string = attributedString.string as NSString
        let range = string.range(of: pattern)

        guard
            range.location != NSNotFound,
            let resultAttributedString = attributedString.mutableCopy() as? NSMutableAttributedString
        else {
            return attributedString
        }

        resultAttributedString.addAttributes(attributes, range: range)

        if includeSeparator, range.upperBound < string.length {
            let punctuationSet = CharacterSet.punctuationCharacters
            let remainingRange = NSRange(location: range.upperBound, length: string.length - range.upperBound)
            let rangeOfPunctuation = string.rangeOfCharacter(
                from: punctuationSet,
                options: [],
                range: remainingRange
            )
            if rangeOfPunctuation.location != NSNotFound {
                resultAttributedString.addAttributes(attributes, range: rangeOfPunctuation)
            }
        }

        return resultAttributedString
    }
}

public final class RangeAttributedStringDecorator: AttributedStringDecoratorProtocol {
    let range: NSRange?
    let attributes: [NSAttributedString.Key: Any]

    public init(attributes: [NSAttributedString.Key: Any], range: NSRange? = nil) {
        self.range = range
        self.attributes = attributes
    }

    public func decorate(attributedString: NSAttributedString) -> NSAttributedString {
        let applicationRange = range ?? NSRange(location: 0, length: attributedString.length)

        guard let resultAttributedString = attributedString.mutableCopy() as? NSMutableAttributedString else {
            return attributedString
        }

        resultAttributedString.addAttributes(attributes, range: applicationRange)
        return resultAttributedString
    }
}

public final class AttributedReplacementStringDecorator: AttributedStringDecoratorProtocol {
    static let marker = "<t_r>"

    let pattern: String
    let replacements: [String]
    let attributes: [NSAttributedString.Key: Any]

    private var customAttributes: [Int: [NSAttributedString.Key: Any]] = [:]

    public init(pattern: String, replacements: [String], attributes: [NSAttributedString.Key: Any]) {
        self.pattern = pattern
        self.replacements = replacements
        self.attributes = attributes
    }

    func addCustomAttributes(for index: Int, attributes: [NSAttributedString.Key: Any]) {
        customAttributes[index] = attributes
    }

    public func decorate(attributedString: NSAttributedString) -> NSAttributedString {
        let string = attributedString.string as NSString
        let components = string.components(separatedBy: pattern)

        let resultAttributedString = NSMutableAttributedString()
        var currentLocation = 0

        for index in 0 ..< components.count {
            let range = NSRange(location: currentLocation, length: components[index].count)
            let attrSubstring = attributedString.attributedSubstring(from: range)
            let replacement = index < replacements.count ? replacements[index] : ""

            let replacementAttributes = attributes.merging(customAttributes[index] ?? [:]) { $1 }

            let attributedReplacement = NSAttributedString(
                string: replacement,
                attributes: replacementAttributes
            )

            resultAttributedString.append(attrSubstring)
            resultAttributedString.append(attributedReplacement)

            currentLocation += components[index].count + pattern.count
        }

        return resultAttributedString
    }
}

public final class CompoundAttributedStringDecorator: AttributedStringDecoratorProtocol {
    let decorators: [AttributedStringDecoratorProtocol]

    public init(decorators: [AttributedStringDecoratorProtocol]) {
        self.decorators = decorators
    }

    public func decorate(attributedString: NSAttributedString) -> NSAttributedString {
        decorators.reduce(attributedString) { result, decorator in
            decorator.decorate(attributedString: result)
        }
    }
}
