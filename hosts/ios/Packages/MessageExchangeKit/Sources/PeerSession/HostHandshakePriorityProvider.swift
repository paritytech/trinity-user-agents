import Foundation

public protocol HostHandshakePriorityProviding {
    func nextPriority() -> UInt64
}

public final class HostHandshakePriorityProvider {
    private let priorityFactory: StatementPriorityMaking
    private var lastPriority: UInt64?

    public init(priorityFactory: StatementPriorityMaking = StatementPriorityFactory()) {
        self.priorityFactory = priorityFactory
    }
}

extension HostHandshakePriorityProvider: HostHandshakePriorityProviding {
    public func nextPriority() -> UInt64 {
        let timestampPriority = priorityFactory.makeTimestampPriority()

        let priority: UInt64 =
            if let lastPriority, timestampPriority <= lastPriority {
                lastPriority + 1
            } else {
                timestampPriority
            }

        lastPriority = priority

        return priority
    }
}
