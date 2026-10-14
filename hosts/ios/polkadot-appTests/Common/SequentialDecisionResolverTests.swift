import Foundation
import Testing

@testable import polkadot_app

// Exercises the generic resolver with throwaway types to prove it carries no domain coupling.
@Suite("SequentialDecisionResolver")
struct SequentialDecisionResolverTests {
    @Test("first gate to yield an outcome wins")
    func firstWins() throws {
        let resolver = SequentialDecisionResolver<Screen>(
            gates: [StubGate { .first }, StubGate { .second }],
            fallback: .fallback
        )

        #expect(try resolver.resolve() == .first)
    }

    @Test("fallback used when no gate yields")
    func fallbackWhenNoYield() throws {
        let resolver = SequentialDecisionResolver<Screen>(
            gates: [StubGate { nil }],
            fallback: .fallback
        )

        #expect(try resolver.resolve() == .fallback)
    }

    @Test("pre-checks run before gates")
    func preChecksRunFirst() throws {
        let resolver = SequentialDecisionResolver<Screen>(
            preChecks: [StubGate { .locked }],
            gates: [StubGate { .first }],
            fallback: .fallback
        )

        #expect(try resolver.resolve() == .locked)
    }

    @Test("a throwing gate aborts resolution")
    func throwingGateAborts() {
        let resolver = SequentialDecisionResolver<Screen>(
            gates: [StubGate { throw StubError.boom }],
            fallback: .fallback
        )

        #expect(throws: StubError.self) {
            try resolver.resolve()
        }
    }
}

private enum Screen: Equatable { case first, second, fallback, locked }

private enum StubError: Error { case boom }

private struct StubGate: DecisionGate {
    let decide: () throws -> Screen?
    func evaluate() throws -> Screen? { try decide() }
}
