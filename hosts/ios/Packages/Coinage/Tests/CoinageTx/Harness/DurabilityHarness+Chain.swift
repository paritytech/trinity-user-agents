import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
@testable import Coinage

/// Whether an action's block is only in the best chain or also finalized.
enum TestActionFinality {
    case finalized
    case inBest
}

/// Denominations a voucher index can carry. More than one so the alias key, built from the
/// denomination and the ring index, is genuinely exercised: with a single value a rule reading the
/// wrong one would still read the right key.
let harnessDenominations = [3, 4]

extension DurabilityHarness {
    /// A function of the index alone, so a shrunk trace replays to the same denominations.
    func denomination(of index: CoinageKeyIndex) -> Int {
        harnessDenominations[Int(index.item) % harnessDenominations.count]
    }

    // MARK: - Asset builders

    func coinInput(_ index: CoinageKeyIndex) -> CoinageTxInput { .coin(.own(index, HarnessKeys.coinKey(index))) }
    func coinOutput(_ index: CoinageKeyIndex) -> OwnAsset { .coin(index, HarnessKeys.coinKey(index)) }
    func voucherInput(_ index: CoinageKeyIndex) -> CoinageTxInput {
        .recyclerVoucher(index, HarnessKeys.voucherMemberKey(index))
    }

    func voucherOutput(_ index: CoinageKeyIndex) -> OwnAsset {
        .recyclerVoucher(index, HarnessKeys.voucherMemberKey(index))
    }

    // MARK: - Moving the chain

    func produceBlock(
        _ finality: TestActionFinality,
        body: [Data] = [],
        mutate: (CoinageChainState) -> CoinageChainState = { $0 }
    ) {
        chain.produceBlock(body: body, mutate: mutate)
        if finality == .finalized { finalizeToBest() }
    }

    func advanceBlocks(_ count: Int, finality: TestActionFinality) {
        for _ in 0 ..< count {
            chain.produceBlock()
        }
        if finality == .finalized { finalizeToBest() }
    }

    func finalizeToBest() {
        chain.finalize(upTo: chain.bestHead.number)
    }

    /// Drops `depth` blocks from the best head. Legal only while they are unfinalized.
    func reorgLastBlocks(_ depth: Int) {
        chain.reorg(depth: depth)
    }

    // MARK: - Arranging chain state

    func mintCoinsOnChain(_ coins: [CoinageKeyIndex], finality: TestActionFinality) {
        produceBlock(finality) { state in
            coins.reduce(state) { $0.mintCoin(HarnessKeys.coinKey($1)) }
        }
    }

    func consumeCoinOnChain(_ coin: CoinageKeyIndex, finality: TestActionFinality) {
        produceBlock(finality) { $0.consumeCoin(HarnessKeys.coinKey(coin)) }
    }

    /// A voucher in a recycler: a member of the denomination's collection, included in a ring, with no
    /// alias state — absence from the map means the alias is available.
    func givenVoucherInRecycler(
        _ voucher: CoinageKeyIndex,
        denomination: Int,
        ring: Int,
        finality: TestActionFinality
    ) {
        let member = HarnessKeys.voucherMemberKey(voucher)
        produceBlock(finality) { $0.joinRecycler(
            member: member,
            exponent: denomination,
            position: .included(ringIndex: ring)
        ) }
    }

    /// Loaded into the recycler but not yet placed in a ring, so it has no ring index and cannot be
    /// unloaded.
    func givenVoucherOnboarding(_ voucher: CoinageKeyIndex, denomination: Int, finality: TestActionFinality) {
        let member = HarnessKeys.voucherMemberKey(voucher)
        produceBlock(finality) { $0.joinRecycler(member: member, exponent: denomination, position: .onboarding) }
    }

    /// Suspended from its ring: still a member, but holding no ring index, so nothing can be said about
    /// whether it was unloaded.
    func givenVoucherSuspended(_ voucher: CoinageKeyIndex, denomination: Int, finality: TestActionFinality) {
        let member = HarnessKeys.voucherMemberKey(voucher)
        produceBlock(finality) { $0.joinRecycler(member: member, exponent: denomination, position: .suspended) }
    }

    /// The queued voucher takes a place in a ring, which is what makes it unloadable.
    func placeVoucherInRing(_ voucher: CoinageKeyIndex, ring: Int, finality: TestActionFinality) {
        let member = HarnessKeys.voucherMemberKey(voucher)
        produceBlock(finality) { state in
            guard let exponent = state.recyclerMembers[member] else { return state }
            return state.joinRecycler(member: member, exponent: exponent, position: .included(ringIndex: ring))
        }
    }

    /// Archival: the membership goes, which is the only thing that takes a voucher out of a recycler.
    func archiveRecyclerOf(_ voucher: CoinageKeyIndex, finality: TestActionFinality) {
        let member = HarnessKeys.voucherMemberKey(voucher)
        produceBlock(finality) { $0.leaveRecycler(member: member) }
    }

    /// The unload executed: the alias reads unloaded at the voucher's current ring.
    func unloadVoucherOnChain(_ voucher: CoinageKeyIndex, finality: TestActionFinality) {
        guard let key = currentAliasKey(index: voucher) else { return }
        produceBlock(finality) { $0.withAlias(key) }
    }

    /// Writes an alias under a ring the voucher is not in, which nothing should read for it.
    func unloadVoucherAtOtherRing(
        _ voucher: CoinageKeyIndex,
        denomination: Int,
        ring: Int,
        finality: TestActionFinality
    ) {
        let key = CoinageChainState.aliasKey(index: voucher, exponent: denomination, ringIndex: ring)
        produceBlock(finality) { $0.withAlias(key) }
    }

    /// Suspends a voucher from its ring: still a recycler member, but holding no ring index, so its alias
    /// can no longer be located. Models a ring rebuild landing on an already-unloaded voucher.
    func suspendVoucher(_ voucher: CoinageKeyIndex, finality: TestActionFinality) {
        let member = HarnessKeys.voucherMemberKey(voucher)
        produceBlock(finality) { state in
            guard let exponent = state.recyclerMembers[member] else { return state }
            return state.joinRecycler(member: member, exponent: exponent, position: .suspended)
        }
    }

    /// Applies an entry's extrinsic in a new block with the given outcome, modelling exactly what the
    /// runtime does to on-chain state: a success mints the outputs, marks spent vouchers unloaded and
    /// removes spent coins; a failure changes nothing this subsystem reads.
    func includeEntry(_ entry: CoinageTxEntry, success: Bool, finality: TestActionFinality) {
        let txHash = entry.submittedAttempt.txHash
        produceBlock(finality, body: [txHash]) { state in
            guard success else { return state.applied(txHash, success: false) }

            var next = state.applied(txHash, success: true)

            for output in entry.outputs {
                switch output {
                case let .coin(_, key):
                    next = next.mintCoin(key)
                case let .recyclerVoucher(index, member):
                    next = next.joinRecycler(member: member, exponent: denomination(of: index), position: .onboarding)
                }
            }

            for input in entry.inputs {
                if case let .recyclerVoucher(index, member) = input,
                   let exponent = state.recyclerMembers[member],
                   let ringIndex = state.ringPositions[member]?.ringIndex {
                    next = next.withAlias(CoinageChainState.aliasKey(
                        index: index,
                        exponent: exponent,
                        ringIndex: ringIndex
                    ))
                }
            }

            for input in entry.inputs where input.isCoin {
                next = next.consumeCoin(input.publicKey)
            }

            return next
        }
    }
}

// MARK: - Registration helpers

extension DurabilityHarness {
    @discardableResult
    func register(
        inputCoin: CoinageKeyIndex,
        outputCoin: CoinageKeyIndex,
        period: UInt32 = harnessMortalPeriod
    ) async throws -> CoinageTxId {
        try await submit([registration(
            inputs: [coinInput(inputCoin)],
            outputs: [coinOutput(outputCoin)],
            period: period
        )])[0]
    }

    @discardableResult
    func registerOffboard(
        inputCoin: CoinageKeyIndex,
        period: UInt32 = harnessMortalPeriod
    ) async throws -> CoinageTxId {
        try await submit([registration(inputs: [coinInput(inputCoin)], outputs: [], period: period)])[0]
    }

    @discardableResult
    func registerSplit(
        inputCoin: CoinageKeyIndex,
        outputCoins: [CoinageKeyIndex],
        period: UInt32 = harnessMortalPeriod
    ) async throws -> CoinageTxId {
        try await submit([registration(
            inputs: [coinInput(inputCoin)],
            outputs: outputCoins.map(coinOutput),
            period: period
        )])[0]
    }

    @discardableResult
    func registerVoucherMint(
        inputCoin: CoinageKeyIndex,
        voucher: CoinageKeyIndex,
        period: UInt32 = harnessMortalPeriod
    ) async throws -> CoinageTxId {
        try await submit([registration(
            inputs: [coinInput(inputCoin)],
            outputs: [voucherOutput(voucher)],
            period: period
        )])[0]
    }

    @discardableResult
    func registerExternalLoad(
        voucher: CoinageKeyIndex,
        period: UInt32 = harnessMortalPeriod
    ) async throws -> CoinageTxId {
        try await submit([registration(inputs: [], outputs: [voucherOutput(voucher)], period: period)])[0]
    }

    @discardableResult
    func registerVoucherUnload(
        vouchers: [CoinageKeyIndex],
        outputCoin: CoinageKeyIndex,
        period: UInt32 = harnessMortalPeriod
    ) async throws -> CoinageTxId {
        try await submit([registration(
            inputs: vouchers.map(voucherInput),
            outputs: [coinOutput(outputCoin)],
            period: period
        )])[0]
    }

    @discardableResult
    func registerGroup(
        pairs: [(input: CoinageKeyIndex, output: CoinageKeyIndex)],
        groupId: CoinageTxGroupId,
        period: UInt32 = harnessMortalPeriod
    ) async throws -> [CoinageTxId] {
        try await submit(pairs.map { pair in
            registration(
                inputs: [coinInput(pair.input)],
                outputs: [coinOutput(pair.output)],
                period: period,
                groupId: groupId
            )
        })
    }
}
