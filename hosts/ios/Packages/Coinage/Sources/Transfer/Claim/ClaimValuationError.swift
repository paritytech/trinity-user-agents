import Foundation

/// A claim could not value its group's outputs because the local coin/voucher store failed to read.
/// Never folded into a verdict: an unreadable store says nothing about what was claimed, so the run
/// ends without one and a later launch, with the store readable, resumes from the durability group.
public struct ClaimValuationError: Error {
    public let underlying: any Error
}
