import Foundation

extension Error {
    var isConnectionError: Bool {
        (self as NSError).domain == NSURLErrorDomain
    }
}
