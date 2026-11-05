import Foundation

enum BackendStatusCode: Int {
    case success = 200
    case created = 201
    case accepted = 202
    case badRequest = 400
    case unauthorize = 401
    case forbidden = 403
    case notFound = 404
    case unprocessableEntry = 422
    case tooManyRequests = 429
    case internalServerError = 500
    case badGateway = 502
    case serverUnavailable = 503

    var isOk: Bool {
        switch self {
        case .success,
             .created,
             .accepted:
            true
        default:
            false
        }
    }
}

struct BackendApiError: Error {
    let statusCode: BackendStatusCode
    let details: String?
}
