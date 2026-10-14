import Foundation

public struct ErrorContent {
    public let title: String
    public let message: String

    public init(title: String, message: String) {
        self.title = title
        self.message = message
    }
}

public protocol ErrorContentConvertible {
    func toErrorContent() -> ErrorContent
}
