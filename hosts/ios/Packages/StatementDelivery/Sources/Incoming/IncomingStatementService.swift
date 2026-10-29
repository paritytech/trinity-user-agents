import AsyncExtensions
import Foundation
import SDKLogger
import StatementStore

public protocol IncomingStatementInput: Sendable {
    var statementId: String { get }
}

public protocol IncomingStatementServicing<Target, Input>: Sendable {
    associatedtype Target
    associatedtype Input: IncomingStatementInput

    /// Inputs published on the targets' topics, for as long as the sequence is iterated.
    func subscribe(to targets: [Target]) -> AnyAsyncSequence<Input>
}

/// Decodes one kind's statements; any account may sign them, so routing is by topic only.
public protocol IncomingStatementDecoding<Target, Input>: Sendable {
    associatedtype Target
    associatedtype Input: IncomingStatementInput

    func topicFilter(for target: Target) throws -> TopicFilter

    func decode(_ statement: Statement, for target: Target) throws -> [Input]
}

public final class IncomingStatementService<Target, Input: IncomingStatementInput>: @unchecked Sendable {
    private let decoder: any IncomingStatementDecoding<Target, Input>
    private let connection: StatementStoreConnecting
    private let logger: SDKLoggerProtocol
    private let workQueue: DispatchQueue

    public init(
        decoder: any IncomingStatementDecoding<Target, Input>,
        connection: StatementStoreConnecting,
        logger: SDKLoggerProtocol,
        workQueue: DispatchQueue = DispatchQueue(label: "io.statement.delivery.incoming.queue")
    ) {
        self.decoder = decoder
        self.connection = connection
        self.logger = logger
        self.workQueue = workQueue
    }
}

extension IncomingStatementService: IncomingStatementServicing {
    public func subscribe(to targets: [Target]) -> AnyAsyncSequence<Input> {
        AsyncStream<Input> { continuation in
            let subscriptions = targets.compactMap { target in
                startSubscription(for: target) { continuation.yield($0) }
            }

            continuation.onTermination = { _ in
                subscriptions.forEach { $0.stop() }
            }
        }
        .eraseToAnyAsyncSequence()
    }
}

private extension IncomingStatementService {
    func startSubscription(for target: Target, onInput: @escaping (Input) -> Void) -> StatementSubscription? {
        do {
            let subscription = try StatementSubscription(
                connection: connection,
                topicFilter: decoder.topicFilter(for: target),
                proofVerifier: StatementPermissiveProofVerifier(),
                workQueue: workQueue,
                logger: logger
            )

            subscription.start { [weak self] statement in
                self?.inputs(in: statement, for: target).forEach(onInput)
                return true
            }

            return subscription
        } catch {
            logger.error("Can't subscribe to incoming statements: \(error)")
            return nil
        }
    }

    func inputs(in statement: Statement, for target: Target) -> [Input] {
        do {
            return try decoder.decode(statement, for: target)
        } catch {
            logger.error("Can't decode incoming statement: \(error)")
            return []
        }
    }
}
