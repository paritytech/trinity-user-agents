import AsyncExtensions
import Foundation
import StatementStore
import Testing

@testable import polkadot_app

@Suite("Statement store status")
struct StatementStoreStatusTests {
    @Test("No network path reads no internet whatever the subscriptions do")
    func noInternetWins() {
        let activity = StatementStoreActivity(live: 3, pending: 0, failed: 0)
        #expect(StatementStoreStatus.resolve(network: .waitingForNetwork, activity: activity) == .noInternet)
    }

    @Test("A connecting socket reads connecting")
    func socketConnecting() {
        #expect(StatementStoreStatus.resolve(network: .connecting, activity: .idle) == .connecting)
    }

    @Test("A streaming subscription reads active even beside a failed one")
    func liveBeatsFailure() {
        let activity = StatementStoreActivity(live: 1, pending: 0, failed: 1)
        #expect(StatementStoreStatus.resolve(network: .connected, activity: activity) == .active)
    }

    @Test("A failed subscription with nothing streaming reads unavailable")
    func failedWithoutLive() {
        let activity = StatementStoreActivity(live: 0, pending: 1, failed: 1)
        #expect(StatementStoreStatus.resolve(network: .connected, activity: activity) == .unavailable)
    }

    @Test("Subscriptions opened but unanswered read connecting")
    func pendingOnly() {
        let activity = StatementStoreActivity(live: 0, pending: 2, failed: 0)
        #expect(StatementStoreStatus.resolve(network: .connected, activity: activity) == .connecting)
    }

    @Test("No subscriptions on a connected socket read active")
    func nothingToSubscribe() {
        #expect(StatementStoreStatus.resolve(network: .connected, activity: .idle) == .active)
    }

    @Test("Ring state follows the status")
    func ringMapping() {
        #expect(StatementStoreStatus.active.connectionState == .connected)
        #expect(StatementStoreStatus.connecting.connectionState == .connecting)
        #expect(StatementStoreStatus.unavailable.connectionState == .offline)
        #expect(StatementStoreStatus.noInternet.connectionState == .offline)
    }
}

@Suite("Statement store activity monitor")
struct StatementStoreActivityMonitorTests {
    struct Failure: Error {}

    @Test("A subscription is pending when opened, live after its first page, gone when closed")
    func lifecycle() async throws {
        let store = MockStatementStore()
        let monitor = StatementStoreActivityMonitor()
        let observed = monitor.observing(store)

        var activities = monitor.activityStream().makeAsyncIterator()
        _ = try await activities.next()

        let stream = try observed.subscribeStatements(with: .matchAll([]))
        let opened = try await activities.next()
        #expect(opened == StatementStoreActivity(live: 0, pending: 1, failed: 0))

        let consumer = Task {
            for try await _ in stream {}
        }
        let answered = try await activities.next()
        #expect(answered == StatementStoreActivity(live: 1, pending: 0, failed: 0))

        consumer.cancel()
        _ = try? await consumer.value
        let closed = try await activities.next()
        #expect(closed == .idle)
    }

    @Test("A subscription nobody iterates is released when it is dropped")
    func droppedWithoutIterating() async throws {
        let store = MockStatementStore()
        let monitor = StatementStoreActivityMonitor()
        let observed = monitor.observing(store)

        var activities = monitor.activityStream().makeAsyncIterator()
        _ = try await activities.next()

        var stream: AnyAsyncSequence<StatementsPage>? = try observed.subscribeStatements(with: .matchAll([]))
        #expect(try await activities.next() == StatementStoreActivity(live: 0, pending: 1, failed: 0))

        stream = nil
        _ = stream
        #expect(try await activities.next() == .idle)
    }

    @Test("A failed subscription stays counted until the next one opens")
    func failureStaysUntilReplaced() async throws {
        let store = MockStatementStore()
        let monitor = StatementStoreActivityMonitor()
        let observed = monitor.observing(store)

        var activities = monitor.activityStream().makeAsyncIterator()
        _ = try await activities.next()

        let stream = try observed.subscribeStatements(with: .matchAll([]))
        _ = try await activities.next()
        let consumer = Task { for try await _ in stream {} }
        _ = try await activities.next()

        store.failSubscriptions(with: Failure())
        await #expect(throws: Failure.self) { try await consumer.value }
        #expect(try await activities.next() == StatementStoreActivity(live: 0, pending: 0, failed: 1))

        _ = try observed.subscribeStatements(with: .matchAll([]))
        #expect(try await activities.next() == StatementStoreActivity(live: 0, pending: 1, failed: 0))
    }

    @Test("A stream that ends silently is a failure too")
    func silentEnd() async throws {
        let store = MockStatementStore()
        let monitor = StatementStoreActivityMonitor()
        let observed = monitor.observing(store)

        var activities = monitor.activityStream().makeAsyncIterator()
        _ = try await activities.next()

        let stream = try observed.subscribeStatements(with: .matchAll([]))
        _ = try await activities.next()
        let consumer = Task { for try await _ in stream {} }
        _ = try await activities.next()

        store.endSubscriptions()
        _ = try await consumer.value
        #expect(try await activities.next() == StatementStoreActivity(live: 0, pending: 0, failed: 1))
    }
}

@Suite("Statement store activity monitor with the real subscription")
struct StatementStoreActivityMonitorSubscriptionTests {
    struct Failure: Error {}

    @Test("The package subscription reads pending, live, and released on stop")
    func startAndStop() async throws {
        let store = MockStatementStore()
        let monitor = StatementStoreActivityMonitor()
        let subscription = makeSubscription(store: store, monitor: monitor)

        var activities = monitor.activityStream().makeAsyncIterator()
        _ = try await activities.next()

        subscription.start { _ in true }
        #expect(try await activities.next() == StatementStoreActivity(live: 0, pending: 1, failed: 0))
        #expect(try await activities.next() == StatementStoreActivity(live: 1, pending: 0, failed: 0))

        subscription.stop()
        #expect(try await activities.next() == .idle)
    }

    @Test("A failing store shows through the package subscription as a failure")
    func failure() async throws {
        let store = MockStatementStore()
        let monitor = StatementStoreActivityMonitor()
        let subscription = makeSubscription(store: store, monitor: monitor)

        var activities = monitor.activityStream().makeAsyncIterator()
        _ = try await activities.next()

        subscription.start { _ in true }
        _ = try await activities.next()
        _ = try await activities.next()

        store.failSubscriptions(with: Failure())
        #expect(try await activities.next() == StatementStoreActivity(live: 0, pending: 0, failed: 1))

        subscription.start { _ in true }
        #expect(try await activities.next() == StatementStoreActivity(live: 0, pending: 1, failed: 0))
    }

    private func makeSubscription(
        store: MockStatementStore,
        monitor: StatementStoreActivityMonitor
    ) -> StatementSubscription {
        StatementSubscription(
            connection: monitor.observing(store),
            topicFilter: .matchAll([]),
            proofVerifier: StatementStoreProofVerifier(logger: nil),
            workQueue: DispatchQueue(label: "statement-store-monitor-tests"),
            logger: nil
        )
    }
}

@Suite("Statement store status service")
struct StatementStoreStatusServiceTests {
    @Test("Joins the socket state with the subscription activity")
    func joinsInputs() async throws {
        let network = MockNetworkStatusService()
        let monitor = StatementStoreActivityMonitor()
        let service = StatementStoreStatusService(
            networkStatusService: network,
            activityMonitor: monitor,
            chainId: "chat",
            logger: StubLogger()
        )

        var statuses = service.statusStream().makeAsyncIterator()
        _ = try await statuses.next()

        service.start()
        network.simulateStatus(.connected)
        #expect(try await statuses.next() == .active)

        network.simulateStatus(.waitingForNetwork)
        #expect(try await statuses.next() == .noInternet)
    }
}
