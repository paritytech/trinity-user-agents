import Foundation

/// A single step in a coin's provenance chain.
public enum Hop: Hashable, Sendable {
    /// The coin changed hands as one of `bundleSize` coins moved together.
    case transfer(bundleSize: UInt8)
    /// The coin was produced by splitting a larger coin into `fanout` pieces.
    case split(fanout: UInt8)
}
