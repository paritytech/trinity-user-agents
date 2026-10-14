import Foundation
import CryptoKit
import SubstrateSdk
import SDKLogger

public struct StatementSubscriptionInit {
    let accountId: AccountId
    let rawSessionId: Data

    public init(accountId: AccountId, rawSessionId: Data) {
        self.accountId = accountId
        self.rawSessionId = rawSessionId
    }
}

public protocol StatementSubscriptionFactoryProtocol {
    func createSubscription(
        for params: StatementSubscriptionInit
    ) throws -> StatementSubscribing

    func createMatchAnySubscription(
        for params: [StatementSubscriptionInit]
    ) throws -> StatementSubscribing
}

public final class StatementSubscriptionFactory {
    let statementStoreFetcher: StatementStoreFetching
    let workQueue: DispatchQueue
    let operationQueue: OperationQueue
    let logger: SDKLoggerProtocol?

    public init(
        statementStoreFetcher: StatementStoreFetching,
        workQueue: DispatchQueue,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol?
    ) {
        self.statementStoreFetcher = statementStoreFetcher
        self.workQueue = workQueue
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension StatementSubscriptionFactory: StatementSubscriptionFactoryProtocol {
    public func createSubscription(
        for params: StatementSubscriptionInit
    ) throws -> StatementSubscribing {
        let proofVerifier = StatementStoreProofVerifier(
            logger: logger
        )
        let rawTopic = try params.rawSessionId.fixedStatementFieldData()

        return StatementSubscription(
            connection: statementStoreFetcher,
            topicFilter: .matchAll([rawTopic]),
            proofVerifier: proofVerifier,
            workQueue: workQueue,
            logger: logger
        )
    }

    public func createMatchAnySubscription(
        for params: [StatementSubscriptionInit]
    ) throws -> StatementSubscribing {
        let rawTopics = try params.map {
            try $0.rawSessionId.fixedStatementFieldData()
        }
        let proofVerifier = StatementStoreProofVerifier(
            logger: logger
        )

        return StatementSubscription(
            connection: statementStoreFetcher,
            topicFilter: .matchAny(rawTopics),
            proofVerifier: proofVerifier,
            workQueue: workQueue,
            logger: logger
        )
    }
}
