import Foundation
import StatementStore
import SDKLogger

protocol PeerSessionPriorityProviding: AnyObject {
    var expiry: UInt64 { get set }
    func initialExpiry(from statements: [Statement]) -> UInt64
    func incrementedExpiry() -> UInt64
}

final class PeerSessionPriorityProvider {
    private let priorityFactory: StatementPriorityMaking
    private let logger: SDKLoggerProtocol?

    var expiry: UInt64 = 0

    init(
        priorityFactory: StatementPriorityMaking = StatementPriorityFactory(),
        logger: SDKLoggerProtocol?
    ) {
        self.priorityFactory = priorityFactory
        self.logger = logger
    }
}

extension PeerSessionPriorityProvider: PeerSessionPriorityProviding {
    func initialExpiry(from statements: [Statement]) -> UInt64 {
        let lastUsedPriority = makeExpiry(from: statements)
        let timestampPriority = priorityFactory.makeTimestampPriority()
        return max(lastUsedPriority + 1, timestampPriority)
    }

    func incrementedExpiry() -> UInt64 {
        let timestampPriority = priorityFactory.makeTimestampPriority()
        let nextPriority = expiry + 1
        expiry = max(nextPriority, timestampPriority)
        logger?.debug("Incremented expiry: \(expiry)")
        return expiry
    }
}

private extension PeerSessionPriorityProvider {
    func makeExpiry(from statements: [Statement]) -> UInt64 {
        var result = UInt64(0)

        for statement in statements {
            if let expiry = statement.getExpiry(), expiry > result {
                result = expiry
            }
        }

        return result
    }
}
