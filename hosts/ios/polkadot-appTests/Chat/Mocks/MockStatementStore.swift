@testable import polkadot_app
import Foundation
import Operation_iOS
import MessageExchangeKit
import StatementStore
import AsyncExtensions
import StructuredConcurrency

final class MockStatementStore {
    class Observer {
        let filter: TopicFilter
        let continuation: AsyncThrowingStream<[Data], Error>.Continuation

        init(filter: TopicFilter, continuation: AsyncThrowingStream<[Data], Error>.Continuation) {
            self.filter = filter
            self.continuation = continuation
        }
    }

    private var statements = Set<MockStatement>()
    private var observers: [Observer] = []
    private let mutex = NSLock()

    func getStatements(with filter: TopicFilter) throws -> [Data] {
        mutex.lock()
        defer { mutex.unlock() }

        return statements
            .filter { filter.matches(topics: $0.topics) }
            .map(\.data)
    }

    func insertStatement(with builder: StatementSubmitParametersBuilding) throws {
        mutex.lock()
        defer { mutex.unlock() }

        let params = try builder.build()

        let statement = try MockStatement(encodedStatement: params.encodedStatement)
        statements.insert(statement)

        for observer in observers where observer.filter.matches(topics: statement.topics) {
            observer.continuation.yield([statement.data])
        }
    }

    func addObserver(_ observer: Observer) {
        mutex.lock()
        defer { mutex.unlock() }

        observers.append(observer)

        let matchingStatements = statements
            .filter { observer.filter.matches(topics: $0.topics) }
            .map(\.data)

        observer.continuation.yield(matchingStatements)
    }

    func removeObserver(_ observer: Observer) {
        mutex.lock()
        defer { mutex.unlock() }

        observers = observers.filter { $0 !== observer }
    }

    func failSubscriptions(with error: Error) {
        takeObservers().forEach { $0.continuation.finish(throwing: error) }
    }

    func endSubscriptions() {
        takeObservers().forEach { $0.continuation.finish() }
    }

    private func takeObservers() -> [Observer] {
        mutex.lock()
        defer { mutex.unlock() }

        let taken = observers
        observers = []
        return taken
    }
}

extension MockStatementStore: StatementStoreSubmitting {
    nonisolated func submitStatement(with builder: StatementSubmitParametersBuilding) async throws {
        try insertStatement(with: builder)
    }
}

extension MockStatementStore: StatementStoreFetching {
    nonisolated func fetchStatements(with filter: TopicFilter) async throws -> [Data] {
        try getStatements(with: filter)
    }

    nonisolated func subscribeStatements(with filter: TopicFilter) -> AnyAsyncSequence<StatementsPage> {
        AsyncThrowingStream { continuation in
            let observer = Observer(filter: filter, continuation: continuation)

            self.addObserver(observer)

            continuation.onTermination = { _ in
                self.removeObserver(observer)
            }
        }
        .map { StatementsPage(statements: $0, isComplete: true) }
        .eraseToAnyAsyncSequence()
    }
}
