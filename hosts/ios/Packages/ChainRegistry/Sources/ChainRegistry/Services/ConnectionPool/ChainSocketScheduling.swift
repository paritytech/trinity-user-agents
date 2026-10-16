import Foundation

/// Runs a `ChainSocket`'s queue hops and deadlines; injectable so tests can fire them deterministically.
public protocol ChainSocketScheduling: Sendable {
    func async(on queue: DispatchQueue, execute work: DispatchWorkItem)
    func schedule(after delay: TimeInterval, on queue: DispatchQueue, execute work: DispatchWorkItem)
}

public struct DispatchChainSocketScheduler: ChainSocketScheduling {
    public init() {}

    public func async(on queue: DispatchQueue, execute work: DispatchWorkItem) {
        queue.async(execute: work)
    }

    public func schedule(after delay: TimeInterval, on queue: DispatchQueue, execute work: DispatchWorkItem) {
        queue.asyncAfter(deadline: .now() + delay, execute: work)
    }
}
