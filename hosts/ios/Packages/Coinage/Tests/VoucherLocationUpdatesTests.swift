import Testing
import Foundation
import Foundation_iOS
import Individuality
@testable import Coinage

@Suite("VoucherLocationService.updates")
struct VoucherLocationUpdatesTests {
    private let enteredAt = Date(timeIntervalSince1970: 1_000)
    private let voucher = Voucher(
        exponent: 5,
        derivationIndex: 0,
        allocatedAt: Date(timeIntervalSince1970: 0),
        readyAt: Date(timeIntervalSince1970: 0),
        publicKey: Data(repeating: 0x71, count: 32)
    )

    struct UnloadedCountCase: Sendable {
        let delivered: UncertainStorage<UInt32?>?
        let expected: UInt32?
    }

    @Test(
        "The delivered unloaded count lands on the in-recycler location",
        arguments: [
            UnloadedCountCase(delivered: .defined(7), expected: 7),
            UnloadedCountCase(delivered: .defined(nil), expected: 0),
            UnloadedCountCase(delivered: .undefined, expected: nil),
            UnloadedCountCase(delivered: nil, expected: nil)
        ]
    )
    func unloadedCountIsWrittenOntoRecycler(_ testCase: UnloadedCountCase) throws {
        // Given
        let location = Voucher.OnChainState.inRecycler(.init(index: 5, membersCount: 3, enteredAt: enteredAt))
        let unloadedCounts: [CoinageKeyIndex: UncertainStorage<UInt32?>] = testCase.delivered.map { [0: $0] } ?? [:]

        // When
        let update = try #require(resolveUpdate(location: location, unloadedCounts: unloadedCounts))

        // Then
        let recycler = try #require(update.remoteState.recycler)
        #expect(recycler.unloadedCount == testCase.expected)
        #expect(recycler.index == 5)
        #expect(recycler.membersCount == 3)
        #expect(recycler.enteredAt == enteredAt)
    }

    struct ScoreCase: Sendable {
        let capacities: [Int16: Int]
        let expectedCurrent: UInt8?
        let expectedMaximum: UInt8?
    }

    @Test(
        "A delivered count is carried regardless of capacity; scores need a known capacity",
        arguments: [
            ScoreCase(capacities: [:], expectedCurrent: nil, expectedMaximum: nil),
            ScoreCase(capacities: [5: 10], expectedCurrent: 17, expectedMaximum: 96)
        ]
    )
    func scoresNeedKnownCapacity(_ testCase: ScoreCase) throws {
        // Given
        let location = Voucher.OnChainState.inRecycler(.init(index: 5, membersCount: 3, enteredAt: enteredAt))

        // When
        let update = try #require(resolveUpdate(
            location: location,
            unloadedCounts: [0: .defined(2)],
            capacities: testCase.capacities
        ))

        // Then
        #expect(update.remoteState.recycler?.unloadedCount == 2)
        #expect(update.recyclerFungibility == testCase.expectedCurrent)
        #expect(update.maxRecyclerFungibility == testCase.expectedMaximum)
    }

    @Test("A non-recycler location passes through with no scores even when a count is delivered")
    func onboardingPassesThroughWithoutScores() throws {
        // Given
        let location = Voucher.OnChainState.onboarding

        // When
        let update = try #require(resolveUpdate(location: location, unloadedCounts: [0: .defined(2)]))

        // Then
        #expect(update.remoteState == .onboarding)
        #expect(update.recyclerFungibility == nil)
        #expect(update.maxRecyclerFungibility == nil)
    }
}

// MARK: - Helpers

private extension VoucherLocationUpdatesTests {
    func resolveUpdate(
        location: Voucher.OnChainState,
        unloadedCounts: [CoinageKeyIndex: UncertainStorage<UInt32?>] = [:],
        capacities: [Int16: Int] = [5: 10]
    ) -> VoucherLocationUpdate? {
        VoucherLocationService.updates(
            locations: [0: location],
            unloadedCounts: unloadedCounts,
            voucherByIndex: [0: voucher],
            capacities: capacities
        )[0]
    }
}
