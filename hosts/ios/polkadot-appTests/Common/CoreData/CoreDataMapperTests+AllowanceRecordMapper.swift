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

        @Test(
            "domain round-trips",
            arguments: [AllowanceRecord.Domain.chat, .device, .product, .coinage]
        )
        func domainRoundTrip(domain: AllowanceRecord.Domain) async throws {
            let original = AllowanceRecord(
                accountId: Data(repeating: 3, count: 32),
                allocatedAt: Date(),
                kind: .statementStore,
                domain: domain,
                priority: .critical,
                latestRenewedPeriod: 7
            )

            try await repo.saveOperation({ [original] }, { [] }).asyncExecute()

            let result = try #require(
                try await repo.fetchOperation(by: { original.identifier }, options: .init()).asyncExecute()
            )
            #expect(result.domain == domain)
        }

        @Test("kind bridging: .statementStore (rawValue 2) round-trips correctly")
        func kindStatementStoreRoundTrip() async throws {
            let testAccountId = Data(repeating: 1, count: 32)
            let original = AllowanceRecord(
                accountId: testAccountId,
                allocatedAt: Date(),
                kind: .statementStore,
                domain: .product,
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
                domain: .product,
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
