import AsyncExtensions
import Foundation
import Testing
@testable import Coinage

@Suite(.timeLimit(.minutes(5)))
struct CoinRecyclingEvaluatorTests {
    private let coin = ExternalPaymentTestFactory.coin(index: 1, age: 4)
    private let database = StubDatabaseDependencyFactory()
    private let settings = StubSwitchableRecyclingStrategySettings(strategy: .maxPrivacy)
    private let recycler = StubCoinageRecyclingService()
    private let evaluator: CoinRecyclingEvaluator

    init() {
        evaluator = CoinRecyclingEvaluator(
            databaseFactory: database,
            settings: settings,
            strategyProvider: StubRecyclingStrategyProvider(),
            ringCapacityProvider: StubRingCapacityProvider(capacities: [:]),
            preClassificator: CoinageAssetPreClassificator(),
            recyclingService: recycler,
            denominationContext: ExternalPaymentTestFactory.denomination,
            logger: nil
        )
    }

    @Test("verdicts keep updating while a recycle waits on the installation backup")
    func verdictsUpdateWhileRecycleWaits() async throws {
        recycler.hold()
        database.trackedCoins.send([ExternalPaymentTestFactory.tracked(coin)])
        evaluator.start()
        defer { evaluator.stop() }

        try await firstValue(of: recycler.subscribeSubmissions()) { $0.count == 1 }
        #expect(evaluator.currentVerdicts()?[coin.derivationIndex] == .toRecycle)

        settings.select(.minPrivacy)

        try await firstValue(of: evaluator.verdicts) { $0[coin.derivationIndex] == .allowUse }
    }
}

private extension CoinRecyclingEvaluatorTests {
    func firstValue<Element>(
        of sequence: AnyAsyncSequence<Element>,
        where predicate: (Element) -> Bool
    ) async throws {
        for try await value in sequence where predicate(value) {
            return
        }
        Issue.record("sequence finished before the expected value")
    }
}

private final class StubSwitchableRecyclingStrategySettings: CoinageRecyclingStrategyProviding, @unchecked Sendable {
    private let subject: AsyncCurrentValueSubject<RecyclingStrategyType>

    var strategy: RecyclingStrategyType {
        subject.value
    }

    init(strategy: RecyclingStrategyType) {
        subject = AsyncCurrentValueSubject(strategy)
    }

    func strategyStream() -> AnyAsyncSequence<RecyclingStrategyType> {
        subject.eraseToAnyAsyncSequence()
    }

    func save(strategy: RecyclingStrategyType) {
        select(strategy)
    }

    func select(_ strategy: RecyclingStrategyType) {
        subject.send(strategy)
    }
}
