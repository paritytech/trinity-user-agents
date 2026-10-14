import Foundation
import BandersnatchApi
import SubstrateSdk
import Individuality
import ExtrinsicService
import StructuredConcurrency
import BigInt
import SDKLogger
import Operation_iOS

public protocol VoucherLoaderProtocol {
    /// Loads vouchers for `amount`. `groupId` labels the durability entries so a caller (e.g.
    /// `ClaimAssetService`) can watch the whole load settle together; `nil` leaves them ungrouped.
    func load(
        amount: BigUInt,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId?
    ) async throws -> [Voucher]
}

public final class VoucherLoader: VoucherLoaderProtocol {
    private let instanceId: CoinageInstanceId
    private let accountId: AccountId
    private let origin: any ExtrinsicOriginDefining
    private let minter: any VoucherMinting
    private let keypairFactory: any VoucherKeyDeriving
    private let txService: any CoinageTxServicing
    private let runtimeService: RuntimeCodingServiceProtocol
    private let logger: (any SDKLoggerProtocol)?

    init(
        instanceId: CoinageInstanceId,
        accountId: AccountId,
        origin: any ExtrinsicOriginDefining,
        minter: any VoucherMinting,
        keypairFactory: any VoucherKeyDeriving,
        txService: any CoinageTxServicing,
        runtimeService: RuntimeCodingServiceProtocol,
        logger: SDKLoggerProtocol?
    ) {
        self.instanceId = instanceId
        self.accountId = accountId
        self.origin = origin
        self.minter = minter
        self.keypairFactory = keypairFactory
        self.txService = txService
        self.runtimeService = runtimeService
        self.logger = logger
    }

    /// Registers the unpaid-load batches through the durability layer: an output-only entry per
    /// batch (`load_recycler_with_external_asset_unpaid_batch` — inputs ∅, outputs 1..N vouchers,
    /// per `durability.md`). Registration claims the minted vouchers durably and returns; the
    /// tracker and recovery pass resolve the on-chain outcome, so this does not await inclusion.
    public func load(
        amount: BigUInt,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId?
    ) async throws -> [Voucher] {
        let denominations = try breakdownContext.breakdown(amountInPlanks: amount)

        let pairs = try await runtimeCalls(for: denominations)

        logger?.debug("Breakdown \(amount) into \(pairs.count) calls")

        guard !pairs.isEmpty else { return [] }

        let maxBatchSize = try await runtimeService.fetchConstant(
            path: CoinagePallet.Constants.maxBatchUnpaidLoad(),
            type: UInt32.self
        )

        let chunkSize = Int(maxBatchSize)
        let chunks = stride(from: 0, to: pairs.count, by: chunkSize).map {
            Array(pairs[$0 ..< min($0 + chunkSize, pairs.count)])
        }

        let requests = chunks.map { chunk in
            CoinageTxRequest(
                inputs: [],
                outputs: chunk.map { .recyclerVoucher($0.0.derivationIndex, $0.0.publicKey) },
                builder: { [instanceId] builder in
                    let batchCall = CoinagePallet.Calls.LoadExternalAssetUnpaidBatch(
                        instanceId: instanceId,
                        items: chunk.map(\.1)
                    )

                    return try builder.adding(call: batchCall.callAsFunction())
                },
                origin: origin
            )
        }

        _ = try await txService.submitTransactions(requests, groupId: groupId)

        logger?.debug("Registered \(requests.count) unpaid-load batches for \(pairs.count) vouchers")

        return pairs.map(\.0)
    }

    private func runtimeCalls(
        for denominations: [Denomination]
    ) async throws -> [(Voucher, CoinagePallet.Calls.LoadExternalAssetUnpaidBatch.UnpaidLoadInput)] {
        typealias Pair = (Voucher, CoinagePallet.Calls.LoadExternalAssetUnpaidBatch.UnpaidLoadInput)
        return try await withThrowingTaskGroup(of: Pair.self) { group in
            denominations.forEach { denomination in
                group.addTask {
                    let voucher = try await self.minter.mintVoucher(exponent: denomination.exponent)
                    let publicKey = voucher.publicKey
                    let keyManager = try self.keypairFactory.createKeyManager(for: voucher)

                    let proof = try keyManager.sign(self.accountId)

                    let input = CoinagePallet.Calls.LoadExternalAssetUnpaidBatch.UnpaidLoadInput(
                        value: voucher.exponent,
                        preservation: .expendable,
                        memberKey: publicKey,
                        proofOfOwnership: proof
                    )

                    return (voucher, input)
                }
            }

            var results: [Pair] = []
            for try await result in group {
                results.append(result)
            }
            return results
        }
    }
}

extension ExtrinsicSubmissionParams {
    static var empty: Self {
        ExtrinsicSubmissionParams(feeAssetId: nil, eventsMatcher: nil)
    }
}

extension ExtrinsicIndexedSubmissionParams {
    static var empty: Self {
        ExtrinsicIndexedSubmissionParams(feeAssetId: nil, eventsMatcher: nil)
    }
}
