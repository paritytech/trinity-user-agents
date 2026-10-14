import Foundation

public extension [StatementField] {
    func sortedByIndex() -> [StatementField] {
        sorted { $0.scaleIndex < $1.scaleIndex }
    }
}
