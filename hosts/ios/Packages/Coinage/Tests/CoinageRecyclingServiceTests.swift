import DurableTransactionsTestSupport
import Testing
import Foundation
@testable import Coinage

/// `CoinageRecyclingService` is submission-only: the decision of *which* coins to recycle lives in
/// `CoinRecyclingEvaluator`. These tests pin the submission contract — one `loadRecyclerWithCoin`
/// entry per coin, consuming that coin and minting a voucher — and that a pre-submission failure
/// surfaces with nothing registered.
@Suite("CoinageRecyclingService Tests")
struct CoinageRecyclingServiceTests {
    @Test("No coins is a no-op")
    func emptyIsNoOp() async throws {
        let sut = makeSUT()

        try await sut.service.recycleCoins([], groupId: nil)

        #expect(sut.txService.submittedInputs.isEmpty)
    }

    @Test("Each coin registers one entry consuming it and minting a voucher")
    func eachCoinRegistersOneEntry() async throws {
        let sut = makeSUT()

        try await sut.service.recycleCoins([coin(index: 7), coin(index: 9)], groupId: nil)

        let inputs = sut.txService.submittedInputs
        let outputs = sut.txService.submittedOutputs
        #expect(inputs == [[.coin(.own(7, key(7)))], [.coin(.own(9, key(9)))]])
        #expect(outputs.allSatisfy { $0.count == 1 })
    }

    @Test("Coins are submitted in the order given")
    func preservesOrder() async throws {
        let sut = makeSUT()

        try await sut.service.recycleCoins([coin(index: 3), coin(index: 1), coin(index: 2)], groupId: nil)

        let inputs = sut.txService.submittedInputs
        #expect(inputs == [[.coin(.own(3, key(3)))], [.coin(.own(1, key(1)))], [.coin(.own(2, key(2)))]])
    }

    @Test("A coin whose preparation fails is skipped, not rethrown, and nothing is submitted")
    func prepareFailureIsSkipped() async throws {
        let sut = makeSUT(minterError: StubError.boom)

        try await sut.service.recycleCoins([coin(index: 7)], groupId: nil)

        #expect(sut.txService.submittedInputs.isEmpty)
    }

    @Test("recycleCoins returns the number of extrinsics actually submitted")
    func returnsSubmittedCount() async throws {
        let sut = makeSUT()

        let submitted = try await sut.service.recycleCoins(
            [coin(index: 7), coin(index: 9), coin(index: 11)],
            groupId: nil
        )

        #expect(submitted == 3)
    }

    @Test("recycleCoins returns zero when nothing is submitted")
    func returnsZeroWhenNothingSubmitted() async throws {
        #expect(try await makeSUT().service.recycleCoins([], groupId: nil) == 0)
        // A coin whose preparation fails is skipped, so the batch submits nothing.
        #expect(try await makeSUT(minterError: StubError.boom).service
            .recycleCoins([coin(index: 7)], groupId: nil) == 0)
    }

    @Test("A group that already has entries is re-joined: nothing is submitted a second time")
    func groupIsRegisteredOnce() async throws {
        let sut = makeSUT()

        let first = try await sut.service.recycleCoins([coin(index: 7)], groupId: "external-payment:p:recycle")
        let second = try await sut.service.recycleCoins([coin(index: 9)], groupId: "external-payment:p:recycle")

        #expect(first == 1)
        #expect(second == 0)
        #expect(sut.txService.submittedInputs == [[.coin(.own(7, key(7)))]])
    }

    @Test("observeRecycling folds the group into allRecycled with the minted vouchers")
    func observeRecyclingReportsMintedVouchers() async throws {
        let voucherService = StubVoucherService()
        let sut = makeSUT(voucherService: voucherService)
        try await sut.service.recycleCoins([coin(index: 7)], groupId: "g")
        let mintedIndices = try await sut.txService.getOperationGroupStatuses("g")
            .flatMap(\.outputs)
            .compactMap { output -> CoinageKeyIndex? in
                guard case let .recyclerVoucher(index, _) = output else { return nil }
                return index
            }
        voucherService.set(vouchers: mintedIndices.map { voucher(index: $0) })

        var last: RecyclingStatus?
        for try await status in sut.service.observeRecycling(groupId: "g") {
            last = status
            if case .allRecycled = status { break }
        }

        guard case let .allRecycled(vouchers, finalized) = last else {
            Issue.record("expected allRecycled, got \(String(describing: last))")
            return
        }
        #expect(finalized)
        #expect(Set(vouchers.map(\.voucher.derivationIndex)) == Set(mintedIndices))
    }
}

// MARK: - SUT

private extension CoinageRecyclingServiceTests {
    struct SUT {
        let service: CoinageRecyclingService
        let txService: MockCoinageTxService
    }

    func makeSUT(minterError: Error? = nil, voucherService: StubVoucherService = StubVoucherService()) -> SUT {
        let txService = MockCoinageTxService()
        let service = CoinageRecyclingService(
            voucherMinter: StubVoucherMinter(error: minterError),
            coinKeypairFactory: StubCoinKeyFactory(),
            voucherKeypairFactory: StubVoucherKeyFactory(),
            txService: txService,
            voucherService: voucherService,
            originFactory: StubOriginFactory(),
            backgroundExecutor: StubBackgroundExecutor(),
            logger: StubLogger()
        )
        return SUT(service: service, txService: txService)
    }

    func key(_ index: CoinageKeyIndex) -> Data {
        Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
    }

    func coin(index: CoinageKeyIndex) -> Coin {
        Coin(exponent: 3, derivationIndex: index, age: 14, isOnchain: true, publicKey: key(index))
    }

    func voucher(index: CoinageKeyIndex) -> Voucher {
        Voucher(
            exponent: 3,
            derivationIndex: index,
            allocatedAt: Date(timeIntervalSince1970: 0),
            readyAt: .distantPast,
            remoteState: .inRecycler(Voucher.Recycler(index: 1, membersCount: 8)),
            publicKey: key(index)
        )
    }
}
