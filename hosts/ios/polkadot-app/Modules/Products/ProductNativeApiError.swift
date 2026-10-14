import Foundation

enum ProductNativeApiError: Error, LocalizedError {
    case chatBotMissing
    case notImplemented
    case signingRejected
    case messagesNotSupported
    case paymentsNotSupported
    case accountServicesNotSupported
    case navigationForbidden
    case notificationsNotAllowed
    case permissionDenied
    case scheduleLimitReached
    case invalidParam(String)
    case notConnected

    var errorDescription: String? {
        switch self {
        case .chatBotMissing:
            "Product bot was deallocated"
        case .notImplemented:
            "Not yet implemented"
        case .signingRejected:
            "Signing request was rejected by the user"
        case .messagesNotSupported:
            "Messages are not supported"
        case .paymentsNotSupported:
            "Payments are not supported in this context"
        case .accountServicesNotSupported:
            "Account services are not supported in this context"
        case .navigationForbidden:
            "Navigation is not allowed in this context"
        case .notificationsNotAllowed:
            "Notifications are not allowed in this context"
        case .permissionDenied:
            "Permissions were denied in this context"
        case .scheduleLimitReached:
            "Schedule limit reached (maximum 64)"
        case let .invalidParam(name):
            "Invalid parameter: \(name)"
        case .notConnected:
            "NotConnected"
        }
    }
}
