import Foundation

protocol URLHandlingServiceProtocol: AnyObject {
    func handle(url: URL) -> Bool
}

final class URLHandlingService {
    let children: [URLHandlingServiceProtocol]

    init(children: [URLHandlingServiceProtocol]) {
        self.children = children
    }
}

extension URLHandlingService: URLHandlingServiceProtocol {
    @discardableResult
    func handle(url: URL) -> Bool {
        for child in children {
            // swiftlint:disable:next for_where
            if child.handle(url: url) {
                return true
            }
        }

        return false
    }
}
