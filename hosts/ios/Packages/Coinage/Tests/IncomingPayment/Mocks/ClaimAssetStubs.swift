import AsyncExtensions
import BigInt
import Foundation
import KeyDerivation
import os
import SubstrateSdk
@testable import Coinage

/// `AssetsTracking` that replays canned balance looks on open and then stays open (a subscription
/// never ends by itself), or fails to open at all. Later looks go out through `send`.
final class StubAssetsTracking: AssetsTracking, @unchecked Sendable {
    struct Unavailable: Error {}

    private struct State {
        var continuations: [AsyncStream<Balance>.Continuation] = []
        var trackCalls = 0
    }

    private let looks: [Balance]
    private let openError: Error?
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(looks: [Balance]) {
        self.looks = looks
        openError = nil
    }

    init(openError: Error) {
        looks = []
        self.openError = openError
    }

    func track(instanceId _: CoinageInstanceId, accountId _: AccountId) async throws -> AnyAsyncSequence<Balance> {
        state.withLock { $0.trackCalls += 1 }
        if let openError { throw openError }

        let looks = looks
        return AsyncStream<Balance> { continuation in
            state.withLock { $0.continuations.append(continuation) }
            looks.forEach { continuation.yield($0) }
        }.eraseToAnyAsyncSequence()
    }

    func send(_ balance: Balance) {
        state.withLock { $0.continuations }.forEach { $0.yield(balance) }
    }

    func trackCalls() -> Int { state.withLock { $0.trackCalls } }
}

/// In-memory `VoucherServiceProtocol`: holds the vouchers a stub loader minted so a claim can value
/// its group's outputs by key. `load` is the real loader's job and is not exercised here.
/// `fetchError` makes every read fail, as a store that cannot be opened would.
final class InMemoryVoucherService: VoucherServiceProtocol, @unchecked Sendable {
    struct Unsupported: Error {}
    struct Unreadable: Error {}

    private let vouchers = OSAllocatedUnfairLock(initialState: [PublicKey: Voucher]())
    var fetchError: Error?

    func save(_ minted: [Voucher]) {
        vouchers.withLock { store in
            minted.forEach { store[$0.publicKey] = $0 }
        }
    }

    func load(
        amount _: BigUInt,
        externalAssetHolder _: any WalletManaging,
        breakdownContext _: DenominationBreakdownContext,
        groupId _: CoinageTxGroupId?
    ) async throws -> [Voucher] {
        throw Unsupported()
    }

    func fetchAllTracked() async throws -> [TrackedVoucher] { [] }

    func fetchTracked(derivationIndices _: Set<CoinageKeyIndex>) async throws -> [TrackedVoucher] { [] }

    func fetchVouchers(publicKeys: Set<PublicKey>) async throws -> [Voucher] {
        if let fetchError { throw fetchError }
        return vouchers.withLock { store in publicKeys.compactMap { store[$0] } }
    }
}

/// A loader factory whose loader does what the real one does minus the chain: breaks `amount` into
/// denominations, mints a deterministic voucher per denomination, registers them as one output-only
/// entry under `groupId` with the tx service, and stores them so they value. `loadError` fails a load
/// before anything is registered. Records every requested amount.
final class StubVoucherLoaderFactory: VoucherLoaderFactoryProtocol, VoucherLoaderProtocol, @unchecked Sendable {
    struct Failure: Error {}

    private struct State {
        var loads: [Balance] = []
        var nextIndex: CoinageKeyIndex = 1_000
    }

    private let vouchers: InMemoryVoucherService
    private let txService: any CoinageTxServicing
    private let state = OSAllocatedUnfairLock(initialState: State())
    var loadError: Error?

    init(vouchers: InMemoryVoucherService, txService: any CoinageTxServicing) {
        self.vouchers = vouchers
        self.txService = txService
    }

    func makeLoader(for _: any WalletManaging) throws -> VoucherLoaderProtocol { self }

    func load(
        amount: BigUInt,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId?
    ) async throws -> [Voucher] {
        state.withLock { $0.loads.append(amount) }
        if let loadError { throw loadError }

        let denominations = try breakdownContext.breakdown(amountInPlanks: amount)
        guard !denominations.isEmpty else { return [] }

        let minted = denominations.map { denomination in
            let index = state.withLock { state in
                defer { state.nextIndex = state.nextIndex.next() }
                return state.nextIndex
            }
            return Voucher(
                exponent: denomination.exponent,
                derivationIndex: index,
                allocatedAt: Date(),
                readyAt: Date(),
                publicKey: testKey(index)
            )
        }
        vouchers.save(minted)

        let request = CoinageTxRequest(
            inputs: [],
            outputs: minted.map { .recyclerVoucher($0.derivationIndex, $0.publicKey) },
            builder: { $0 },
            origin: StubExtrinsicOrigin()
        )
        _ = try await txService.submitTransactions([request], groupId: groupId)
        return minted
    }

    func loads() -> [Balance] { state.withLock { $0.loads } }
}

/// In-memory `CoinServiceProtocol` for valuing a group's coin outputs by key.
final class InMemoryCoinService: CoinServiceProtocol, @unchecked Sendable {
    private let coins = OSAllocatedUnfairLock(initialState: [PublicKey: Coin]())

    init(coins seed: [Coin] = []) {
        coins.withLock { store in
            seed.forEach { store[$0.publicKey] = $0 }
        }
    }

    func fetchAllTrackedCoins() async throws -> [TrackedCoin] { [] }

    func fetchCoins(publicKeys: Set<PublicKey>) async throws -> Set<Coin> {
        coins.withLock { store in Set(publicKeys.compactMap { store[$0] }) }
    }

    func save(coins minted: [Coin]) async throws {
        coins.withLock { store in
            minted.forEach { store[$0.publicKey] = $0 }
        }
    }
}
