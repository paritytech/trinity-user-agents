import Foundation

struct SequentialDecisionResolver<Outcome>: DecisionResolver {
    private let preChecks: [any DecisionGate<Outcome>]
    private let gates: [any DecisionGate<Outcome>]
    private let fallback: Outcome

    init(
        preChecks: [any DecisionGate<Outcome>] = [],
        gates: [any DecisionGate<Outcome>],
        fallback: Outcome
    ) {
        self.preChecks = preChecks
        self.gates = gates
        self.fallback = fallback
    }

    func resolve() throws -> Outcome {
        if let outcome = try firstOutcome(in: preChecks) {
            return outcome
        }

        if let outcome = try firstOutcome(in: gates) {
            return outcome
        }

        return fallback
    }
}

private extension SequentialDecisionResolver {
    func firstOutcome(in gates: [any DecisionGate<Outcome>]) throws -> Outcome? {
        try gates
            .lazy
            .compactMap { try $0.evaluate() }
            .first
    }
}
