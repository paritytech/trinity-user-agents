import Foundation

public extension MessageExchange {
    enum InitializationError: Error {
        case statementDecodingFailed
        case statementPayloadDecryptionFailed
        case statementPayloadDecodingFailed
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
