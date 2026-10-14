import Foundation
import Operation_iOS
import SubstrateSdk
import SDKLogger
import AsyncExtensions
import StructuredConcurrency

public protocol StatementStoreFetching {
    func fetchStatements(with filter: TopicFilter) async throws -> [Data]
    func subscribeStatements(with filter: TopicFilter) throws -> AnyAsyncSequence<StatementsPage>
}

public protocol StatementStoreSubmitting {
    func submitStatement(with builder: StatementSubmitParametersBuilding) async throws
}

public typealias StatementStoreConnecting = StatementStoreFetching & StatementStoreSubmitting

public final class StatementStoreConnection {
    struct NewStatementsData: Decodable {
        let statements: [String]
        let remaining: Int?
    }

    struct SubscriptionResult: Decodable {
        let event: String
        let data: NewStatementsData
    }

    typealias SubscriptionUpdate = JSONRPCSubscriptionUpdate<SubscriptionResult>

    private let connection: JSONRPCEngine
    private let retryMatcher: StatementSubmitErrorMatching
    private let logger: SDKLoggerProtocol?
    private let retryDelay: TimeInterval
    private let retryCount: Int
    private let fetchTimeout: Int

    public init(
        connection: JSONRPCEngine,
        retryMatcher: StatementSubmitErrorMatching,
        retryDelay: TimeInterval = 2,
        retryCount: Int = 10,
        fetchTimeout: Int = 2,
        logger: SDKLoggerProtocol?
    ) {
        self.connection = connection
        self.retryMatcher = retryMatcher
        self.retryDelay = retryDelay
        self.retryCount = retryCount
        self.fetchTimeout = fetchTimeout
        self.logger = logger
    }

    deinit {
        logger?.debug("Deinit")
    }
}

extension StatementStoreConnection: StatementStoreFetching {
    public func fetchStatements(with filter: TopicFilter) async throws -> [Data] {
        let stream = try performSubscription(with: filter)

        var accumStatements: [Data] = []

        for try await result in stream {
            for statement in result.data.statements {
                let data = try Data(hexString: statement)
                accumStatements.append(data)
            }

            let remaning = result.data.remaining ?? 0

            if remaning == 0 {
                break
            }
        }

        return accumStatements
    }

    public func subscribeStatements(with filter: TopicFilter) throws -> AnyAsyncSequence<StatementsPage> {
        try performSubscription(with: filter)
            .map { result in
                let statements = try result.data.statements.compactMap { statement in
                    try Data(hexString: statement)
                }
                let isComplete = (result.data.remaining ?? 0) == 0
                return StatementsPage(statements: statements, isComplete: isComplete)
            }
            .eraseToAnyAsyncSequence()
    }
}

extension StatementStoreConnection: StatementStoreSubmitting {
    public func submitStatement(with builder: StatementSubmitParametersBuilding) async throws {
        var retryAttemps: Int = retryCount

        while true {
            try Task.checkCancellation()

            let rpcOperation = JSONRPCOperation<StatementSubmitParameters, StatementSubmitResult.RawModel>(
                engine: connection,
                method: RPCMethod.submit,
                timeout: Constant.submitTimeout
            )

            rpcOperation.parameters = try builder.build()

            do {
                logger?.debug("Submitting statement")

                let rawResult = try await rpcOperation.asyncExecute()
                try StatementSubmitResult(rawModel: rawResult).ensureSuccess()

                logger?.debug("Statement successfully submitted")

                return
            } catch let error where retryMatcher.match(error: error) && retryAttemps > 0 {
                logger?.error("Retrying after error: \(error)")
                retryAttemps -= 1

                try await Task.sleep(for: .seconds(retryDelay))
            }
        }
    }
}

private extension StatementStoreConnection {
    func performSubscription(with filter: TopicFilter) throws
        -> AnyAsyncSequence<SubscriptionResult> {
        connection.asyncSubscribe(
            RPCMethod.subscribe,
            params: StatementSubscriptionParams(topicFilter: filter),
            unsubscribeMethod: RPCMethod.unsubscribe
        )
        .map { (update: SubscriptionUpdate) -> SubscriptionResult in
            update.params.result
        }
        .eraseToAnyAsyncSequence()
    }
}

private extension StatementStoreConnection {
    enum RPCMethod {
        static let submit = "statement_submit"
        static let subscribe = "statement_subscribeStatement"
        static let unsubscribe = "statement_unsubscribeStatement"
    }

    enum Constant {
        static let submitTimeout = 5
    }

    func submitStatementRetryMatcher(for error: Error) -> Bool {
        retryMatcher.match(error: error)
    }
}
