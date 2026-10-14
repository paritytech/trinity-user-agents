import Foundation

public protocol StatementPriorityMaking {
    func makeTimestampPriority() -> UInt64
}

public final class StatementPriorityFactory {
    public init() {}
}

extension StatementPriorityFactory: StatementPriorityMaking {
    public func makeTimestampPriority() -> UInt64 {
        let now = UInt64(Date().timeIntervalSince1970)
        let priority = max(0, now - Constant.unixOffset)
        let expiry = 0xFFFF_FFFF_0000_0000 | priority
        return expiry
    }
}

private extension StatementPriorityFactory {
    enum Constant {
        static let unixOffset = UInt64(1_763_164_800)
    }
}
