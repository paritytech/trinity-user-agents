import Coinage
import Foundation
import Testing

@testable import polkadot_app

@Suite("Coinage installation repository")
struct CoinageInstallationRepositoryTests {
    private let repository = CoinageInstallationCoreDataRepository(storageFacade: UserDataStorageTestFacade())

    @Test("a fresh store holds no previous installations")
    func emptyByDefault() async throws {
        let previous = try await repository.getPrevious()

        #expect(previous.isEmpty)
    }

    @Test("previous installations are recorded once, repeats are skipped")
    func addPrevious() async throws {
        try await repository.addPrevious([.other, .other])
        try await repository.addPrevious([.other, .fixed(0x22)])

        let previous = try await repository.getPrevious()
        #expect(previous.map(\.id) == [.other, .fixed(0x22)].sorted { $0.hex < $1.hex })
        #expect(previous.allSatisfy { !$0.initialScanCompleted })
        #expect(previous.allSatisfy { $0.coinScanNextIndex == 0 && $0.voucherScanNextIndex == 0 })
    }

    @Test("scan cursors and the completion flag are persisted per installation")
    func cursors() async throws {
        try await repository.addPrevious([.other, .fixed(0x22)])

        try await repository.updateCoinScanNextIndex(1_500, for: .other)
        try await repository.updateVoucherScanNextIndex(2_000, for: .other)
        try await repository.markInitialScanCompleted(.other)

        let previous = try await repository.getPrevious()
        let updated = try #require(previous.first { $0.id == .other })
        let untouched = try #require(previous.first { $0.id == .fixed(0x22) })

        #expect(updated.coinScanNextIndex == 1_500)
        #expect(updated.voucherScanNextIndex == 2_000)
        #expect(updated.initialScanCompleted)
        #expect(untouched.coinScanNextIndex == 0)
        #expect(!untouched.initialScanCompleted)
    }

    @Test("confirming covers every installation known then; one recorded later starts unconfirmed")
    func markAllUserConfirmed() async throws {
        try await repository.addPrevious([.other, .fixed(0x22)])
        try await repository.markInitialScanCompleted(.other)

        try await repository.markAllUserConfirmed()
        try await repository.addPrevious([.fixed(0x33)])

        let previous = try await repository.getPrevious()
        let confirmed = previous.filter(\.isUserConfirmedCompletion).map(\.id)
        #expect(Set(confirmed) == [.other, .fixed(0x22)])
        #expect(previous.first { $0.id == .fixed(0x33) }?.isUserConfirmedCompletion == false)
        #expect(previous.allSatisfy { !$0.awaitsUserConfirmation })
    }

    @Test("an update for an unknown installation fails")
    func unknownInstallation() async throws {
        await #expect(throws: CoinageInstallationRepositoryError.self) {
            try await repository.markInitialScanCompleted(.other)
        }
    }
}
