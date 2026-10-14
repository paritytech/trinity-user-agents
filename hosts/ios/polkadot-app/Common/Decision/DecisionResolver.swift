import Foundation

protocol DecisionResolver<Outcome> {
    associatedtype Outcome

    func resolve() throws -> Outcome
}
