import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import KeyDerivation
import Individuality
import Revive
import SDKLogger
import SubstrateSdk

public protocol InstallationRegistrationSubmitting: Sendable {
    func submitAttempt(target: InstallationRegistrationTarget) async throws -> DurableTxId
}

/// Estimates the fee of an extrinsic on the chain the `AccountDataStore` contract lives on.
public protocol RegistrationFeeEstimating: Sendable {
    func estimateFee(_ builder: @escaping ExtrinsicBuilderClosure, origin: any ExtrinsicOriginDefining) async throws
        -> BigUInt
}

public struct DataStoreAccountUnmappedError: Error, Equatable {
    public init() {}
}

public enum InstallationRegistrationError: Error, Equatable {
    case nothingSubmitted
}

/// One registration attempt: funds and checks the data store account, simulates the call, has the fee
/// and deposit covered, then hands the extrinsic to the durable engine under the target's group.
final class InstallationRegistrationSubmitter: InstallationRegistrationSubmitting, @unchecked Sendable {
    /// The declared limits are what the fee is charged against, so they carry a margin rather than the
    /// pallet maximum.
    private static let limitMarginPercent: BigUInt = 20

    private let dataStoreRepository: any AccountDataStoreRepositoryProtocol
    private let reviveApi: any ReviveContractApiProtocol
    private let pgasProvisioner: any PGASAccountProvisioning
    private let feeEstimator: any RegistrationFeeEstimating
    private let callArguments: any ReviveCallArgumentsProviding
    private let chainId: ChainId
    private let originFactory: any OriginCreating
    private let engine: any DurableTxServicing
    private let logger: (any SDKLoggerProtocol)?

    init(
        dataStoreRepository: any AccountDataStoreRepositoryProtocol,
        reviveApi: any ReviveContractApiProtocol,
        pgasProvisioner: any PGASAccountProvisioning,
        feeEstimator: any RegistrationFeeEstimating,
        callArguments: any ReviveCallArgumentsProviding,
        chainId: ChainId,
        originFactory: any OriginCreating,
        engine: any DurableTxServicing,
        logger: (any SDKLoggerProtocol)?
    ) {
        self.dataStoreRepository = dataStoreRepository
        self.reviveApi = reviveApi
        self.pgasProvisioner = pgasProvisioner
        self.feeEstimator = feeEstimator
        self.callArguments = callArguments
        self.chainId = chainId
        self.originFactory = originFactory
        self.engine = engine
        self.logger = logger
    }

    func submitAttempt(target: InstallationRegistrationTarget) async throws -> DurableTxId {
        let call = try await dataStoreRepository.registrationCall(target: target)

        try await pgasProvisioner.ensureFunded(account: call.account.accountId)
        try await requireMapped(call)

        let cost = try await estimateCost(call)
        try await pgasProvisioner.ensureCovers(account: call.account.accountId, required: cost.required)

        return try await register(call, cost: cost, target: target)
    }
}

private extension InstallationRegistrationSubmitter {
    struct RegistrationCost {
        let builder: ExtrinsicBuilderClosure
        let required: BigUInt
    }

    /// A dry-run maps the origin for the length of the simulation, so only storage tells whether a real
    /// call would pass.
    func requireMapped(_ call: InstallationRegistrationCall) async throws {
        guard try await reviveApi.isAccountMapped(call.account.accountId) else {
            throw DataStoreAccountUnmappedError()
        }
    }

    func estimateCost(_ call: InstallationRegistrationCall) async throws -> RegistrationCost {
        let dryRun = try await reviveApi.dryRun(
            origin: call.account.accountId,
            contract: call.contract,
            input: call.input
        )
        let depositLimit = Self.withMargin(dryRun.storageDeposit)
        let reviveCall = RevivePallet.Call(
            dest: call.contract,
            value: .zero,
            weightLimit: Self.withMargin(dryRun.weightRequired),
            storageDepositLimit: depositLimit,
            data: call.input
        )
        let weightLimitArgument = try await callArguments.weightLimitArgument()
        let builder: ExtrinsicBuilderClosure = {
            try reviveCall.add(to: $0, weightLimitArgument: weightLimitArgument)
        }

        let fee = try await feeEstimator.estimateFee(builder, origin: origin(for: call))
        let required = depositLimit + fee
        logger?.info(
            "Installation registration: dry-run deposit=\(dryRun.storageDeposit) fee=\(fee) required=\(required)"
        )

        return RegistrationCost(builder: builder, required: required)
    }

    func register(
        _ call: InstallationRegistrationCall,
        cost: RegistrationCost,
        target: InstallationRegistrationTarget
    ) async throws -> DurableTxId {
        logger?.info("Installation registration: calling contract, \(target.logDescription)")

        let ids = try await engine.submitTransactions(
            domain: .coinageInstallation,
            requests: [DurableTxRequest(builder: cost.builder, origin: origin(for: call))],
            groupId: target.registrationGroup
        ) { _, _ in }

        guard let id = ids.first else { throw InstallationRegistrationError.nothingSubmitted }
        logger?.info("Installation registration: submitted as durable tx \(id)")
        return id
    }

    func origin(for call: InstallationRegistrationCall) async throws -> any ExtrinsicOriginDefining {
        let privateKey = call.account.privateKey
        let wallet = DynamicDerivedWallet(secretKeyProvider: { privateKey })
        return try await originFactory.createSignedOrigin(for: wallet, chainId: chainId)
    }

    static func withMargin(_ weight: Substrate.WeightV2) -> Substrate.WeightV2 {
        Substrate.WeightV2(refTime: withMargin(weight.refTime), proofSize: withMargin(weight.proofSize))
    }

    static func withMargin(_ value: BigUInt) -> BigUInt {
        value * (100 + limitMarginPercent) / 100
    }
}
