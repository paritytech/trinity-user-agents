import CoreData
import Foundation
import Testing
@testable import polkadot_app

struct OutgoingStatementPublicationTests {
    private let facade = UserDataStorageTestFacade()

    @Test func setsOverwritesAndDeletesOnePeriodPerStatement() async throws {
        try await facade.databaseService.performWrite { context in
            try context.setOutgoingStatementPeriod(7, for: "A")
            try context.setOutgoingStatementPeriod(8, for: "A")
            try context.setOutgoingStatementPeriod(3, for: "B")
        }

        let periods = try await facade.databaseService.performRead { context in
            try context.outgoingStatementPeriods(for: ["A", "B", "C"])
        }

        #expect(periods == ["A": 8, "B": 3])

        try await facade.databaseService.performWrite { context in
            try context.deleteOutgoingStatementPublication(for: "A")
        }

        let afterDelete = try await facade.databaseService.performRead { context in
            try context.outgoingStatementPeriod(for: "A")
        }

        #expect(afterDelete == nil)
    }
}
