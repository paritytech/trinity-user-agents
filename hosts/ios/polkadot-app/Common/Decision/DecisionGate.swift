import Foundation

protocol DecisionGate<Outcome> {
    associatedtype Outcome

    func evaluate() throws -> Outcome?
}
