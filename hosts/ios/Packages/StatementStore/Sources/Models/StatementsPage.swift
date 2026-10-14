import Foundation

public struct StatementsPage {
    public let statements: [Data]
    public let isComplete: Bool

    public init(statements: [Data], isComplete: Bool) {
        self.statements = statements
        self.isComplete = isComplete
    }
}
