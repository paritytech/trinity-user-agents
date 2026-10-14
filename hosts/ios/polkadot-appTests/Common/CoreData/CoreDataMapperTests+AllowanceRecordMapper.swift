import Foundation
import Individuality
import Operation_iOS
import SubstrateSdk
import Testing

@testable import polkadot_app

extension CoreDataMapperTests {
    @Suite("AllowanceRecordMapper")
    struct AllowanceRecordMapperTests {
        private let facade = UserDataStorageTestFacade()
        private var repo: AnyDataProviderRepository<AllowanceRecord> {
            facade.makeRepo(mapper: AllowanceRecordMapper())
        }

        @Test("kind bridging: .statementStore (rawValue 2) round-trips correctly")
        func kindStatementStoreRoundTrip() async throws {
            let testAccountId = Data(repeating: 1, count: 32)
            let original = AllowanceRecord(
                accountId: testAccountId,
                allocatedAt: Date(),
                kind: .statementStore,
                priority: .normal,
                latestRenewedPeriod: nil
            )

            try await repo.saveOperation({ [original] }, { [] }).asyncExecute()

            let result = try #require(
                try await repo.fetchOperation(by: { original.identifier }, options: .init()).asyncExecute()
            )
            #expect(result.kind == .statementStore)
        }

        @Test("kind bridging: nil kind stored as -1 and restored to nil")
        func kindNilRoundTrip() async throws {
            let testAccountId = Data(repeating: 2, count: 32)
            let original = AllowanceRecord(
                accountId: testAccountId,
                allocatedAt: Date(),
                kind: nil,
                priority: .normal,
                latestRenewedPeriod: nil
            )

            try await repo.saveOperation({ [original] }, { [] }).asyncExecute()

            let result = try #require(
                try await repo.fetchOperation(by: { original.identifier }, options: .init()).asyncExecute()
            )
            #expect(result.kind == nil)
        }
    }
}
