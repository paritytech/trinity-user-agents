import Foundation
import SubstrateSdk

extension ChatRequest {
    static let statementStoreContext = Data("chat-request".utf8)

    struct PaginationDay {
        let day: UInt64
        let remainedTillNext: TimeInterval
    }

    static func paginationDay(from date: Date) -> PaginationDay? {
        let relativeTimestamp = TimeInterval(1_763_164_800)
        let dateTimestamp = date.timeIntervalSince1970
        let secondsInDay: TimeInterval = 86_400

        let timestampDiff = dateTimestamp - relativeTimestamp

        guard timestampDiff >= 0 else {
            return nil
        }

        let day = UInt64(timestampDiff / secondsInDay)
        let reminedTillNext = TimeInterval(day + 1) * secondsInDay - timestampDiff

        return PaginationDay(day: day, remainedTillNext: reminedTillNext)
    }

    static func paginationTopic(from accountId: AccountId, day: UInt64) throws -> Data {
        let scaleEncoder = ScaleEncoder()
        try ChatRequest.statementStoreContext.encode(scaleEncoder: scaleEncoder)
        try accountId.encode(scaleEncoder: scaleEncoder)
        try day.encode(scaleEncoder: scaleEncoder)

        return try scaleEncoder.encode().blake2b32()
    }

    static func allPeerStatementsTopic(from accountId: AccountId) throws -> Data {
        let scaleEncoder = ScaleEncoder()
        try ChatRequest.statementStoreContext.encode(scaleEncoder: scaleEncoder)
        try accountId.encode(scaleEncoder: scaleEncoder)

        return try scaleEncoder.encode().blake2b32()
    }
}
