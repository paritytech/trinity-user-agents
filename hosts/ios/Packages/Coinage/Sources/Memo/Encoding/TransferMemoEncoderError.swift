import Foundation

/// Errors that can occur during transfer memo encoding.
public enum TransferMemoEncoderError: Error {
    case encodingFailed(underlying: Error)
}
