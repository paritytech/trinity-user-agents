import BigInt
import ExtrinsicService
import Foundation
import FoundationExt
import KeyDerivation
import SDKLogger
import SubstrateSdk
import SubstrateOperation
import SubstrateSdkExt

/// Executes the offboarding flow: one extrinsic per recycler group, all registered atomically under
/// the payment id so the payment never leaves a half-committed group behind and a crash re-entry
/// re-joins the surviving group instead of re-registering claimed vouchers.
/// The transaction layer tracks the spent voucher inputs and surplus outputs; the surplus vouchers are
/// already persisted by the allocator when minted, so no local state bookkeeping is required.
///
/// Each group's call must independently satisfy the pallet invariant:
/// `input_value (= coin_value * alias_count) == external_asset_amount + sum(loaded_coin_values)`
///
/// Groups with surplus use `unload_recycler_into_external_asset_and_loaded_coins`.
/// Groups without surplus use `unload_recycler_into_external_asset`.
final class OffboardVouchersForPaymentService {
    private let instanceId: CoinageInstanceId
    private let voucherKeyFactory: any VoucherKeyDeriving
    private let voucherService: VoucherServiceProtocol
    private let voucherMinter: any VoucherMinting
    private let recyclerLoader: RecyclerReadinessLoading
    private let txService: any CoinageTxServicing
    private let originFactory: OriginCreating
    private let quotaTracker: any UnloadQuotaTracking
    private let blockNumberProvider: BlockInfoProviding
    private let denominationContext: DenominationBreakdownContext
    private let dateProvider: any DateProviding
    private let logger: SDKLoggerProtocol?

    init(
        instanceId: CoinageInstanceId,
        voucherKeyFactory: any VoucherKeyDeriving,
        voucherService: VoucherServiceProtocol,
        voucherMinter: any VoucherMinting,
        recyclerLoader: RecyclerReadinessLoading,
        txService: any CoinageTxServicing,
        originFactory: OriginCreating,
        quotaTracker: any UnloadQuotaTracking,
        blockNumberProvider: BlockInfoProviding,
        denominationContext: DenominationBreakdownContext,
        dateProvider: any DateProviding,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.instanceId = instanceId
        self.voucherKeyFactory = voucherKeyFactory
        self.voucherService = voucherService
        self.voucherMinter = voucherMinter
        self.recyclerLoader = recyclerLoader
        self.txService = txService
        self.originFactory = originFactory
        self.quotaTracker = quotaTracker
        self.blockNumberProvider = blockNumberProvider
        self.denominationContext = denominationContext
        self.dateProvider = dateProvider
        self.logger = logger
    }

    /// `surplus` is what `vouchers` exceed the payment by; it is folded back into fresh vouchers.
    func execute(
        payment: ExternalPayment,
        vouchers: [Voucher],
        surplus: Balance
    ) async throws -> OffboardOutcome {
        try await executeSubmissions(payment: payment, vouchers: vouchers, surplus: surplus)
    }
}

/// The unload's single verdict, folded from its per-group entries. `partialSuccess` is not a
/// failure — money did move, just not all of it: `settledInPlanks` is what the finalized groups
/// delivered, and the payment ends there.
enum OffboardOutcome: Equatable {
    case success
    case partialSuccess(settledInPlanks: Balance, executed: Int, total: Int)
    case failed
}

// MARK: - Submission Pipeline

private extension OffboardVouchersForPaymentService {
    func executeSubmissions(
        payment: ExternalPayment,
        vouchers: [Voucher],
        surplus: Balance
    ) async throws -> OffboardOutcome {
        let groupId = groupId(for: payment)

        try await registerOrRejoinGroup(payment: payment, vouchers: vouchers, surplus: surplus, groupId: groupId)

        return try await awaitGroupOutcome(groupId: groupId)
    }

    func groupId(for payment: ExternalPayment) -> CoinageTxGroupId {
        payment.identifier
    }

    /// Registers the whole payment as one atomic durability group, or re-joins the group a prior
    /// attempt already registered. Re-registering is impossible after a crash — the vouchers are
    /// claimed by the surviving entries — so on re-entry we adopt the group and await it.
    func registerOrRejoinGroup(
        payment: ExternalPayment,
        vouchers: [Voucher],
        surplus: Balance,
        groupId: CoinageTxGroupId
    ) async throws {
        let existing = try await txService.getOperationGroupStatuses(groupId)
        guard existing.isEmpty else {
            logger?.debug("Rejoining offboard group \(groupId): \(existing.count) entries")
            return
        }

        // Validation only guards the registration path — a re-join adopts already-committed inputs.
        guard !vouchers.isEmpty else { throw OffboardVouchersForPaymentError.emptyVouchers }
        guard !vouchers.contains(where: { $0.recycler == nil }) else {
            throw OffboardVouchersForPaymentError.missingRecyclerInfo
        }

        let requests = try await buildGroupRequests(payment: payment, vouchers: vouchers, surplus: surplus)
        _ = try await txService.submitTransactions(requests, groupId: groupId)
        logger?.debug("Registered \(requests.count) offboard groups under \(groupId)")

        // Each group spent one free-unload token; noted only on this registration path (a re-join
        // adopts already-committed inputs, whose tokens the original attempt already spent) and after
        // submission, so the quota estimate follows the actual spend.
        await quotaTracker.noteUnloadHappened(count: requests.count)
    }

    func awaitGroupOutcome(groupId: CoinageTxGroupId) async throws -> OffboardOutcome {
        for try await entries in txService.subscribeOperationGroupStatuses(groupId) {
            guard !entries.isEmpty, !entries.contains(where: \.status.isLive) else {
                continue
            }

            let finalized = entries.filter { $0.status == .finalizedSuccess }
            let total = entries.count

            if finalized.count == total {
                return .success
            } else if !finalized.isEmpty {
                return try await .partialSuccess(
                    settledInPlanks: settledValue(of: finalized),
                    executed: finalized.count,
                    total: total
                )
            } else {
                return .failed
            }
        }

        throw OffboardVouchersForPaymentError.subscriptionEnded
    }

    func buildGroupRequests(
        payment: ExternalPayment,
        vouchers: [Voucher],
        surplus: Balance
    ) async throws -> [CoinageTxRequest] {
        let details = try await buildGroupDetails(groups: unloadCalls(for: vouchers), surplus: surplus)

        let blockHash = try await blockNumberProvider.fetchCurrentHash()

        let origins = try await originFactory.createAsUnloadTokenOrigins(
            voucherGroups: details.map(\.group.vouchers),
            currentDate: dateProvider.read(),
            blockHash: blockHash
        )

        // Several calls can share a recycler, so the revision query is deduplicated.
        var seenKeys = Set<RecyclerKey>()
        let revisions = try await recyclerLoader.fetchRevisions(
            for: details.map(\.group.key).filter { seenKeys.insert($0).inserted },
            blockHash: blockHash
        )

        return try zip(details, origins).map { detail, origin in
            guard let revision = revisions[detail.group.key] else {
                throw OffboardVouchersForPaymentError.unexpectedEmptyRevision(detail.group.key)
            }

            return try buildRequest(GroupSubmission(
                details: detail,
                revision: revision,
                destination: payment.destination,
                origin: origin
            ))
        }
    }
}

// MARK: - Settled Value

private extension OffboardVouchersForPaymentService {
    /// What the finalized groups delivered to the destination: each group's voucher inputs minus the
    /// surplus vouchers it minted back. Only the vouchers the entries name are read, by public key.
    func settledValue(of entries: [CoinageTxEntry]) async throws -> Balance {
        let inputKeys = entries.flatMap(\.inputs).compactMap { input -> (CoinageKeyIndex, PublicKey)? in
            guard case let .recyclerVoucher(index, key) = input else { return nil }
            return (index, key)
        }
        let outputKeys = entries.flatMap(\.outputs).compactMap { output -> (CoinageKeyIndex, PublicKey)? in
            guard case let .recyclerVoucher(index, key) = output else { return nil }
            return (index, key)
        }
        let vouchers = try await voucherService.fetchVouchers(publicKeys: Set((inputKeys + outputKeys).map(\.1)))
        let exponents = Dictionary(
            vouchers.map { ($0.derivationIndex, $0.exponent) },
            uniquingKeysWith: { first, _ in first }
        )

        func value(of index: CoinageKeyIndex) throws -> Balance {
            guard let exponent = exponents[index] else {
                throw OffboardVouchersForPaymentError.unknownVoucher(index)
            }
            return denominationContext.valueInPlanks(for: exponent)
        }

        return try entries.reduce(Balance(0)) { partial, entry in
            let inputs = try entry.inputs.reduce(Balance(0)) { sum, input in
                guard case let .recyclerVoucher(index, _) = input else { return sum }
                return try sum + value(of: index)
            }
            let outputs = try entry.outputs.reduce(Balance(0)) { sum, output in
                guard case let .recyclerVoucher(index, _) = output else { return sum }
                return try sum + value(of: index)
            }
            return partial + (inputs > outputs ? inputs - outputs : 0)
        }
    }
}

// MARK: - Models

private extension OffboardVouchersForPaymentService {
    struct GroupDetails {
        let group: RecyclerVoucherChunk
        let externalAssetAmount: Balance
        let surplusVouchers: [Voucher]
    }

    struct GroupSubmission {
        let details: GroupDetails
        let revision: UInt32
        let destination: AccountId
        let origin: any ExtrinsicOriginDefining
    }

    /// The single call carrying the whole payment's surplus, and the change vouchers minted for it.
    ///
    /// Identified by its position, not by its recycler: a recycler holding more vouchers than one
    /// call may unload is split across several calls that all share the same `RecyclerKey`.
    struct SurplusHost {
        let hostIndex: Int
        let surplusVouchers: [Voucher]
    }
}

// MARK: - Per-Group Calculation

private extension OffboardVouchersForPaymentService {
    func buildGroupDetails(
        groups: [RecyclerVoucherChunk],
        surplus: Balance
    ) async throws -> [GroupDetails] {
        let host = try await resolveSurplusHost(groups: groups, surplus: surplus)

        return groups.enumerated().map { index, group in
            let isHost = index == host?.hostIndex
            return GroupDetails(
                group: group,
                externalAssetAmount: isHost ? groupInput(group) - surplus : groupInput(group),
                surplusVouchers: isHost ? host?.surplusVouchers ?? [] : []
            )
        }
    }

    /// Picks the first call large enough to host the entire surplus and mints the change vouchers
    /// for it. `nil` when there is no surplus; throws when no single call can carry it.
    func resolveSurplusHost(
        groups: [RecyclerVoucherChunk],
        surplus: Balance
    ) async throws -> SurplusHost? {
        guard surplus > 0 else { return nil }

        guard let hostIndex = groups.firstIndex(where: { groupInput($0) >= surplus }) else {
            throw OffboardVouchersForPaymentError.noSurplusHost(surplus)
        }

        let surplusVouchers = try await allocateSurplusVouchers(surplus: surplus)
        return SurplusHost(hostIndex: hostIndex, surplusVouchers: surplusVouchers)
    }

    func groupInput(_ group: RecyclerVoucherChunk) -> Balance {
        group.vouchers.reduce(Balance(0)) {
            $0 + denominationContext.valueInPlanks(for: $1.exponent)
        }
    }
}

// MARK: - Grouping

private extension OffboardVouchersForPaymentService {
    /// One call per recycler, split further when a recycler holds more vouchers than the pallet
    /// accepts as aliases in a single call.
    func unloadCalls(for vouchers: [Voucher]) async throws -> [RecyclerVoucherChunk] {
        let maxPerCall = try await max(Int(recyclerLoader.maxConsolidation()), 1)
        return try RecyclerVoucherChunker.chunk(vouchers, maxPerChunk: maxPerCall)
    }
}

// MARK: - Surplus

private extension OffboardVouchersForPaymentService {
    func allocateSurplusVouchers(surplus: Balance) async throws -> [Voucher] {
        guard surplus > 0 else { return [] }

        // Broken down in planks: the surplus is already one, and the round trip through `Decimal`
        // does not survive `toSubstrateAmount` intact — $0.07 at precision 18 comes back ten planks
        // heavy, which the breakdown then cannot place.
        let denominations = try denominationContext.breakdown(amountInPlanks: surplus)
        return try await voucherMinter.mintVouchers(denominations.map(\.exponent))
    }
}

// MARK: - Submission

private extension OffboardVouchersForPaymentService {
    func buildRequest(_ submission: GroupSubmission) throws -> CoinageTxRequest {
        let aliases = try submission.details.group.vouchers.map {
            try voucherKeyFactory.createKeyManager(for: $0)
                .deriveAlias(for: UnloadTokenContextBuilder.recyclerAliasContext)
        }

        let key = submission.details.group.key

        return CoinageTxRequest(
            inputs: submission.details.group.vouchers.map { .recyclerVoucher($0.derivationIndex, $0.publicKey) },
            outputs: submission.details.surplusVouchers
                .map { .recyclerVoucher($0.derivationIndex, $0.publicKey) },
            builder: { builder in
                if submission.details.surplusVouchers.isEmpty {
                    let call = self.buildExternalAssetCall(aliases: aliases, key: key, submission: submission)
                    return try builder.adding(call: call.callAsFunction())
                } else {
                    let call = try self.buildExternalAssetAndVouchersCall(
                        aliases: aliases,
                        key: key,
                        submission: submission
                    )

                    return try builder.adding(call: call.callAsFunction())
                }
            },
            origin: submission.origin
        )
    }

    func buildExternalAssetCall(
        aliases: [Data],
        key: RecyclerKey,
        submission: GroupSubmission
    ) -> CoinagePallet.Calls.UnloadRecyclerIntoExternalAsset {
        CoinagePallet.Calls.UnloadRecyclerIntoExternalAsset(
            instanceId: instanceId,
            aliases: aliases,
            value: Int8(key.exponent),
            index: key.index,
            revision: submission.revision,
            to: submission.destination
        )
    }

    func buildExternalAssetAndVouchersCall(
        aliases: [Data],
        key: RecyclerKey,
        submission: GroupSubmission
    ) throws -> CoinagePallet.Calls.UnloadRecyclerIntoExternalAssetAndLoadedCoins {
        let loadedCoinEntries = submission.details.surplusVouchers.map { voucher in
            CoinagePallet.Calls.UnloadRecyclerIntoExternalAssetAndLoadedCoins.LoadedCoin(
                coinValue: Int8(voucher.exponent),
                memberKey: voucher.publicKey
            )
        }

        return CoinagePallet.Calls.UnloadRecyclerIntoExternalAssetAndLoadedCoins(
            instanceId: instanceId,
            aliases: aliases,
            value: Int8(key.exponent),
            index: key.index,
            revision: submission.revision,
            to: submission.destination,
            externalAssetAmount: submission.details.externalAssetAmount,
            loadedCoins: loadedCoinEntries
        )
    }
}
