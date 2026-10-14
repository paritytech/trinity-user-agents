import AsyncExtensions
import Foundation
import os
import StatementStore

protocol StatementStoreActivityMonitoring: AnyObject {
    func activityStream() -> AnyAsyncSequence<StatementStoreActivity>
    func observing(_ connection: StatementStoreConnecting) -> StatementStoreConnecting
}

final class StatementStoreActivityMonitor: StatementStoreActivityMonitoring, @unchecked Sendable {
    fileprivate enum SubscriptionState {
        case pending
        case live
        case failed
    }

    private let subject = AsyncCurrentValueSubject<StatementStoreActivity>(.idle)
    private let subscriptions = OSAllocatedUnfairLock<[UUID: SubscriptionState]>(initialState: [:])

    func activityStream() -> AnyAsyncSequence<StatementStoreActivity> {
        subject.eraseToAnyAsyncSequence()
    }

    func observing(_ connection: StatementStoreConnecting) -> StatementStoreConnecting {
        ObservedStatementStoreConnection(base: connection, monitor: self)
    }
}

private extension StatementStoreActivityMonitor {
    func opened(_ id: UUID) {
        update { subscriptions in
            subscriptions = subscriptions.filter { $0.value != .failed }
            subscriptions[id] = .pending
        }
    }

    func answered(_ id: UUID) {
        update { $0[id] = .live }
    }

    func failed(_ id: UUID) {
        update { $0[id] = .failed }
    }

    func closed(_ id: UUID) {
        update { subscriptions in
            if subscriptions[id] != .failed {
                subscriptions[id] = nil
            }
        }
    }
}

private extension StatementStoreActivityMonitor {
    func update(_ mutate: (inout [UUID: SubscriptionState]) -> Void) {
        subscriptions.withLock { subscriptions in
            mutate(&subscriptions)

            let activity = StatementStoreActivity(
                live: subscriptions.values.filter { $0 == .live }.count,
                pending: subscriptions.values.filter { $0 == .pending }.count,
                failed: subscriptions.values.filter { $0 == .failed }.count
            )

            if activity != subject.value {
                subject.send(activity)
            }
        }
    }
}

private final class ObservedStatementStoreConnection: StatementStoreConnecting, @unchecked Sendable {
    private let base: StatementStoreConnecting
    private let monitor: StatementStoreActivityMonitor

    init(base: StatementStoreConnecting, monitor: StatementStoreActivityMonitor) {
        self.base = base
        self.monitor = monitor
    }

    func fetchStatements(with filter: TopicFilter) async throws -> [Data] {
        try await base.fetchStatements(with: filter)
    }

    func submitStatement(with builder: StatementSubmitParametersBuilding) async throws {
        try await base.submitStatement(with: builder)
    }

    func subscribeStatements(with filter: TopicFilter) throws -> AnyAsyncSequence<StatementsPage> {
        let upstream = try base.subscribeStatements(with: filter)
        let token = ObservedSubscriptionToken(monitor: monitor)

        return ObservedStatements(upstream: upstream, token: token).eraseToAnyAsyncSequence()
    }
}

private final class ObservedSubscriptionToken: @unchecked Sendable {
    let id = UUID()
    let monitor: StatementStoreActivityMonitor

    init(monitor: StatementStoreActivityMonitor) {
        self.monitor = monitor
        monitor.opened(id)
    }

    deinit {
        monitor.closed(id)
    }
}

private struct ObservedStatements: AsyncSequence, @unchecked Sendable {
    typealias Element = StatementsPage

    let upstream: AnyAsyncSequence<StatementsPage>
    let token: ObservedSubscriptionToken

    func makeAsyncIterator() -> Iterator {
        Iterator(upstream: upstream.makeAsyncIterator(), token: token)
    }

    struct Iterator: AsyncIteratorProtocol {
        var upstream: AnyAsyncSequence<StatementsPage>.AsyncIterator
        let token: ObservedSubscriptionToken
        var answered = false

        mutating func next() async throws -> StatementsPage? {
            do {
                guard let page = try await upstream.next() else {
                    if Task.isCancelled {
                        token.monitor.closed(token.id)
                    } else {
                        token.monitor.failed(token.id)
                    }
                    return nil
                }

                if !answered {
                    answered = true
                    token.monitor.answered(token.id)
                }

                return page
            } catch {
                if Task.isCancelled || error is CancellationError {
                    token.monitor.closed(token.id)
                } else {
                    token.monitor.failed(token.id)
                }
                throw error
            }
        }
    }
}
