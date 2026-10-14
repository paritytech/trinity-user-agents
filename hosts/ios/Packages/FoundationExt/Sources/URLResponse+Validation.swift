import Foundation

public struct HTTPResponseError: Error, LocalizedError {
    public let statusCode: Int

    public init(statusCode: Int) {
        self.statusCode = statusCode
    }

    public var errorDescription: String? {
        "Request failed with status code: \(statusCode)"
    }
}

public extension URLResponse {
    func ensureSuccess() throws {
        guard let httpResponse = self as? HTTPURLResponse else {
            throw HTTPResponseError(statusCode: -1)
        }

        guard (200 ... 299).contains(httpResponse.statusCode) else {
            throw HTTPResponseError(statusCode: httpResponse.statusCode)
        }
    }
}
