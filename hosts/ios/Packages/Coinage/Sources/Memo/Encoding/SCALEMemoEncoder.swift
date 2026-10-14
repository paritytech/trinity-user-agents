import Foundation
import SubstrateSdk

/// SCALE encoder for TransferCoinEntry values.
/// Wraps the existing ScaleCodable conformance for standalone encoding.
public final class SCALEMemoEncoder: TransferMemoEncoding {
    public init() {}

    public func encode(_ entry: TransferCoinEntry) throws -> Data {
        do {
            return try entry.scaleEncoded()
        } catch {
            throw TransferMemoEncoderError.encodingFailed(underlying: error)
        }
    }
}
