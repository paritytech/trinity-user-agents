import Foundation

/// Key for deduplicating recycler queries.
public struct RecyclerKey: Hashable {
    public let exponent: Int16
    public let index: UInt32

    public init(exponent: Int16, index: UInt32) {
        self.exponent = exponent
        self.index = index
    }
}
