import Foundation

public enum StatementSubscriptionError: Error {
    case statementDecodingFailed
    case other(Error)
}
