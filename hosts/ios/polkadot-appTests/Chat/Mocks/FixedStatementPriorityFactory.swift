import MessageExchangeKit

struct FixedStatementPriorityFactory: StatementPriorityMaking {
    let timestampPriority: UInt64

    func makeTimestampPriority() -> UInt64 {
        timestampPriority
    }
}
