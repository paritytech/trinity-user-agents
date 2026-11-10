import Clocks
import Foundation
@testable import StatementDelivery
import Testing

@Suite(.timeLimit(.minutes(10)))
struct OutgoingStatementDeliveryServiceTests {
    private let input = TestInput(statementId: "A")
    private let store = FakeOutgoingStore()
    private let resolver = FakeResolver()
    private let service = FakeOutgoingService()
    private let postDelivery = RecordingPostDelivery()

    @Test func publishesWithResolvedSignerAndRecordsPeriod() async {
        store.awaitFirstDelivery(of: "A")

        await makeDelivery().deliverUntilPublished(input)

        #expect(service.publishes.map(\.signer) == [resolver.signerAccount(for: "A", period: 10)])
        #expect(store.published.map(\.period) == [10])
        #expect(postDelivery.delivered == ["A"])
    }

    @Test func runsPostDeliveryOnlyOnFirstPublish() async throws {
        let delivery = makeDelivery()
        let signer = StubSigner(keyId: "x")

        try await delivery.deliverOnce(input, signer: signer, period: 10)
        try await delivery.deliverOnce(input, signer: signer, period: 11)

        #expect(store.published.map(\.period) == [10, 11])
        #expect(postDelivery.delivered == ["A"])
    }

    @Test func marksOversizedFailedWithoutResolvingSigner() async {
        store.awaitFirstDelivery(of: "A")
        resolver.setMaxSize(99)

        await makeDelivery().deliverUntilPublished(input)

        #expect(store.failed == ["A"])
        #expect(resolver.calls.isEmpty)
        #expect(service.publishes.isEmpty)
    }

    @Test func marksTerminalResolutionFailedAndStops() async {
        store.awaitFirstDelivery(of: "A")
        resolver.fail("A", with: TestTerminalError())

        await makeDelivery().deliverUntilPublished(input)

        #expect(store.failed == ["A"])
        #expect(service.publishes.isEmpty)
    }

    @Test func retriesTransientFailuresUntilPublished() async {
        store.awaitFirstDelivery(of: "A")
        service.failNext(2)

        await makeDelivery().deliverUntilPublished(input)

        #expect(service.publishes.count == 1)
        #expect(resolver.calls.count == 3)
    }

    @Test func doesNothingOnceNoLongerAwaitingFirstDelivery() async {
        await makeDelivery().deliverUntilPublished(input)

        #expect(resolver.calls.isEmpty)
        #expect(service.publishes.isEmpty)
    }
}

private extension OutgoingStatementDeliveryServiceTests {
    func makeDelivery() -> OutgoingStatementDeliveryService<TestInput> {
        OutgoingStatementDeliveryService(
            dependencies: OutgoingStatementDeliveryDependencies(
                store: store,
                resolver: resolver,
                service: service,
                builder: FakeBuilder(),
                postDelivery: postDelivery
            ),
            execution: OutgoingStatementDeliveryExecution(
                backgroundExecutor: InlineExecutor(),
                clock: ImmediateClock()
            ),
            diagnostics: .silent
        )
    }
}
