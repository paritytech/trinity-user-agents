import Foundation

protocol URLHandlingServiceProtocol: AnyObject {
    func handle(url: URL) -> Bool

    /// The same, for a link whose arrival was read earlier, where the system handed it over. A handler that
    /// times what the link opens implements it; the rest ignore the arrival.
    func handle(url: URL, arrival: LinkArrival) -> Bool
}

extension URLHandlingServiceProtocol {
    func handle(url: URL, arrival _: LinkArrival) -> Bool {
        handle(url: url)
    }
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

    @discardableResult
    func handle(url: URL, arrival: LinkArrival) -> Bool {
        for child in children {
            // swiftlint:disable:next for_where
            if child.handle(url: url, arrival: arrival) {
                return true
            }
        }

        return false
    }
}
