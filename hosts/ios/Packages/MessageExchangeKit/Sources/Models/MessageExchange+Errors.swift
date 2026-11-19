import Foundation

public extension MessageExchange {
    /// Only polling can fail initialization: undecodable statements are skipped, since anyone can publish them.
    enum InitializationError: Error {
        case statementDecodingFailed
        case other(Error)
    }

    enum IncomingMessageError: Error {
        case decryptionFailed
        case decodingFailed
    }

    enum OutgoingMessageError: Error {
        case failedToPost(Error)
        case gotFailedResponse(MessageExchange.ResponseCode)
    }

    enum AddToQueueError: Error {
        case messageTooBig(maxSize: Int, actualSize: Int)
        case encryptionFailed
        case encodingFailed
        case unsupportedRoute(PeerSessionRoute)
    }
}
