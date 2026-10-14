import BigInt
import Coinage
import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

extension CoreDataMapperTests {
    @Suite("VoucherLocationMapper")
    struct VoucherLocationMapperTests {
        private let facade = UserDataStorageTestFacade()
        private let minimumMembers: UInt32 = 32
        private let tenMinutes: TimeInterval = 10 * 60
        private var fullRepo: AnyDataProviderRepository<Voucher> { facade.makeRepo(mapper: VoucherMapper()) }
        private var locationRepo: AnyDataProviderRepository<VoucherLocationUpdate> {
            facade.makeRepo(mapper: VoucherLocationMapper())
        }

        /// Helper: a voucher with no ring yet, which is how one is minted.
        private func unlocatedVoucher(index: CoinageKeyIndex) -> Voucher {
            let now = Date(timeIntervalSinceReferenceDate: 3_000_000)

            return Voucher(
                exponent: 5,
                derivationIndex: index,
                allocatedAt: now,
                readyAt: now.addingTimeInterval(600),
                remoteState: .unlocated,
                publicKey: Data(repeating: 0x71, count: 32)
            )
        }

        private func stored(_ voucher: Voucher) async throws -> Voucher {
            try #require(
                try await fullRepo
                    .fetchOperation(by: { voucher.identifier }, options: .init())
                    .asyncExecute()
            )
        }

        @Test("captures the ceiling when the voucher first lands in a ring")
        func capturesCeilingOnRingEntry() async throws {
            let original = unlocatedVoucher(index: 300)
            try await fullRepo.saveOperation({ [original] }, { [] }).asyncExecute()

            try await locationRepo.saveOperation({
                [VoucherLocationUpdate(
                    derivationIndex: 300,
                    remoteState: .inRecycler(.init(index: 7, membersCount: 40)),
                    recyclerFungibility: 55,
                    maxRecyclerFungibility: 80
                )]
            }, { [] }).asyncExecute()

            let result = try await stored(original)
            #expect(result.maxRecyclerFungibility == 80)
            #expect(result.recyclerFungibility == 55)
        }

        /// Zero is a legitimate ceiling for a fully drained ring, so it must not be mistaken for
        /// "nothing captured yet" and overwritten by a later, more favourable reading.
        @Test("a ceiling of zero stays frozen when a later reading is positive")
        func zeroCeilingStaysFrozen() async throws {
            let original = unlocatedVoucher(index: 301)
            try await fullRepo.saveOperation({ [original] }, { [] }).asyncExecute()

            let ring = Voucher.OnChainState.inRecycler(.init(index: 9, membersCount: 12))

            // First presence in ring 9: the ring is drained, so the ceiling is genuinely zero.
            try await locationRepo.saveOperation({
                [VoucherLocationUpdate(
                    derivationIndex: 301,
                    remoteState: ring,
                    recyclerFungibility: 0,
                    maxRecyclerFungibility: 0
                )]
            }, { [] }).asyncExecute()

            #expect(try await stored(original).maxRecyclerFungibility == 0)

            // A later look at the same ring reports better numbers — the chain decrements its
            // unloaded count on a failed dispatch. The frozen ceiling must not move.
            try await locationRepo.saveOperation({
                [VoucherLocationUpdate(
                    derivationIndex: 301,
                    remoteState: ring,
                    recyclerFungibility: 42,
                    maxRecyclerFungibility: 90
                )]
            }, { [] }).asyncExecute()

            let result = try await stored(original)
            #expect(result.maxRecyclerFungibility == 0)
            // The current score is not frozen, so it does track the newer reading.
            #expect(result.recyclerFungibility == 42)
        }

        @Test("a ceiling is recaptured when the voucher is placed in a different ring")
        func recapturesCeilingOnRingChange() async throws {
            let original = unlocatedVoucher(index: 302)
            try await fullRepo.saveOperation({ [original] }, { [] }).asyncExecute()

            try await locationRepo.saveOperation({
                [VoucherLocationUpdate(
                    derivationIndex: 302,
                    remoteState: .inRecycler(.init(index: 1, membersCount: 10)),
                    recyclerFungibility: 20,
                    maxRecyclerFungibility: 30
                )]
            }, { [] }).asyncExecute()

            // A different ring is a different anonymity set, so its ceiling is a new fact.
            try await locationRepo.saveOperation({
                [VoucherLocationUpdate(
                    derivationIndex: 302,
                    remoteState: .inRecycler(.init(index: 2, membersCount: 60)),
                    recyclerFungibility: 70,
                    maxRecyclerFungibility: 95
                )]
            }, { [] }).asyncExecute()

            #expect(try await stored(original).maxRecyclerFungibility == 95)
        }

        @Test("a location-only update leaves both scores untouched")
        func locationOnlyUpdateKeepsScores() async throws {
            let original = unlocatedVoucher(index: 303)
            try await fullRepo.saveOperation({ [original] }, { [] }).asyncExecute()

            try await locationRepo.saveOperation({
                [VoucherLocationUpdate(
                    derivationIndex: 303,
                    remoteState: .inRecycler(.init(index: 4, membersCount: 20)),
                    recyclerFungibility: 61,
                    maxRecyclerFungibility: 77
                )]
            }, { [] }).asyncExecute()

            // Same ring, no scores resolved this tick: neither field may be disturbed.
            try await locationRepo.saveOperation({
                [VoucherLocationUpdate(
                    derivationIndex: 303,
                    remoteState: .inRecycler(.init(index: 4, membersCount: 21))
                )]
            }, { [] }).asyncExecute()

            let result = try await stored(original)
            #expect(result.recyclerFungibility == 61)
            #expect(result.maxRecyclerFungibility == 77)
        }

        @Test("Ring growth preserves the first confirmed inclusion and other voucher fields")
        func updatesLocationOnly() async throws {
            let fortyMinutes: TimeInterval = 40 * 60
            let now = Date(timeIntervalSinceReferenceDate: 2_000_000)
            let original = Voucher(
                exponent: 11,
                derivationIndex: 100,
                allocatedAt: now,
                readyAt: now.addingTimeInterval(fortyMinutes),
                remoteState: .unlocated,
                publicKey: Data(repeating: 0x64, count: 32)
            )
            try await fullRepo.saveOperation({ [original] }, { [] }).asyncExecute()

            let updated = VoucherLocationUpdate(
                derivationIndex: 100,
                remoteState: .inRecycler(.init(index: 3, membersCount: minimumMembers - 1, enteredAt: now))
            )

            try await locationRepo.saveOperation({ [updated] }, { [] }).asyncExecute()

            let later = VoucherLocationUpdate(
                derivationIndex: 100,
                remoteState: .inRecycler(.init(
                    index: 3,
                    membersCount: minimumMembers,
                    enteredAt: now.addingTimeInterval(tenMinutes)
                ))
            )
            try await locationRepo.saveOperation({ [later] }, { [] }).asyncExecute()

            let result = try #require(
                try await fullRepo.fetchOperation(by: { original.identifier }, options: .init()).asyncExecute()
            )
            #expect(result == original.adjusting(state: .inRecycler(.init(
                index: 3, membersCount: minimumMembers, enteredAt: now
            ))))
        }

        @Test("Moving rings and restoring restart the inclusion timer")
        func movingAndRestoringRestartTimer() async throws {
            let first = Date(timeIntervalSince1970: 1_000)
            let later = first.addingTimeInterval(tenMinutes)
            let original = Voucher(
                exponent: 1,
                derivationIndex: 0,
                allocatedAt: first,
                readyAt: first,
                remoteState: .inRecycler(.init(index: 1, membersCount: minimumMembers, enteredAt: first)),
                publicKey: Data(repeating: 0, count: 32)
            )
            try await fullRepo.saveOperation({ [original] }, { [] }).asyncExecute()
            let moved = VoucherLocationUpdate(
                derivationIndex: 0,
                remoteState: .inRecycler(.init(index: 2, membersCount: minimumMembers, enteredAt: later))
            )
            try await locationRepo.saveOperation({ [moved] }, { [] }).asyncExecute()
            let afterMove = try await fullRepo.fetchOperation(by: { original.identifier }, options: .init())
                .asyncExecute()
            #expect(afterMove == original.adjusting(state: moved.remoteState))

            let restored = original.adjusting(state: .inRecycler(.init(index: 2, membersCount: 0)))
            try await fullRepo.saveOperation({ [restored] }, { [] }).asyncExecute()
            let afterRestore = try await fullRepo.fetchOperation(by: { original.identifier }, options: .init())
                .asyncExecute()
            #expect(afterRestore == restored)

            let confirmed = VoucherLocationUpdate(
                derivationIndex: 0,
                remoteState: .inRecycler(.init(
                    index: 2,
                    membersCount: minimumMembers,
                    enteredAt: later.addingTimeInterval(tenMinutes)
                ))
            )
            try await locationRepo.saveOperation({ [confirmed] }, { [] }).asyncExecute()
            let afterConfirmation = try await fullRepo.fetchOperation(by: { original.identifier }, options: .init())
                .asyncExecute()
            #expect(afterConfirmation == original.adjusting(state: confirmed.remoteState))
        }

        @Test("throws missingVoucher when entity does not exist")
        func throwsForNewEntity() async throws {
            let update = VoucherLocationUpdate(
                derivationIndex: 999,
                remoteState: .onboarding
            )

            await #expect(throws: VoucherLocationMapper.MappingError.missingVoucher) {
                try await locationRepo.saveOperation({ [update] }, { [] }).asyncExecute()
            }
        }
    }
}
