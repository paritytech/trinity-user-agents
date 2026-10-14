import AsyncExtensions
import Foundation
@testable import Coinage

/// Fixed preset.
struct StubRecyclingStrategySettings: CoinageRecyclingStrategyProviding {
    let strategy: RecyclingStrategyType

    func strategyStream() -> AnyAsyncSequence<RecyclingStrategyType> {
        AsyncStream { $0.yield(strategy); $0.finish() }.eraseToAnyAsyncSequence()
    }

    func save(strategy _: RecyclingStrategyType) {}
}

/// The real parametric strategy for the preset, with no quota decoration.
struct StubRecyclingStrategyProvider: RecyclingStrategyProviding {
    func coinStrategy(
        for type: RecyclingStrategyType,
        mode _: BalanceEvaluationMode
    ) async throws -> CoinRecyclingStrategyProtocol {
        voucherStrategy(for: type)
    }

    func voucherStrategy(for type: RecyclingStrategyType) -> CoinRecyclingStrategyProtocol {
        ParametricRecyclingStrategy(params: type.params(forcedRecyclingAge: CoinageConstants.recycleAtAge))
    }
}

/// Fixed ring capacities.
struct StubRingCapacityProvider: RingCapacityProviding {
    let capacities: [Int16: Int]

    func capacities(for exponents: Set<Int16>) async throws -> [Int16: Int] {
        capacities.filter { exponents.contains($0.key) }
    }

    func peekCapacities(for exponents: Set<Int16>) async -> [Int16: Int] {
        capacities.filter { exponents.contains($0.key) }
    }
}
