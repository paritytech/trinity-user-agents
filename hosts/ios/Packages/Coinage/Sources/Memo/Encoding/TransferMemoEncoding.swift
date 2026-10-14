import Foundation

/// Protocol for encoding TransferCoinEntry values.
public protocol TransferMemoEncoding {
    func encode(_ entry: TransferCoinEntry) throws -> Data
}
