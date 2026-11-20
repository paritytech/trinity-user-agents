import Foundation
import SubstrateSdk
import SubstrateSdkExt
import ExtrinsicService
import SubstrateOperation
import ChainStore
import SDKLogger

public enum SlotReplacementValidity: Equatable {
    case valid
    case invalid(InvalidTransaction)
    /// The node could not answer; the submission decides.
    case inconclusive
}

/// The holder's recovery window is not readable from storage, so only a dry run can tell whether the slot
/// may be replaced now.
public protocol SlotReplacementValidating {
    func validate(
        call: any RuntimeCallable,
        origin: ExtrinsicOriginDefining,
        chainId: ChainId
    ) async throws -> SlotReplacementValidity
}

public final class SlotReplacementValidator: SlotReplacementValidating {
    private let operationFactory: ExtrinsicOperationFactoryProtocol
    private let validationApi: TaggedTransactionQueueApiProtocol
    private let blockInfoProvider: BlockInfoProviding
    private let logger: SDKLoggerProtocol

    public init(
        operationFactory: ExtrinsicOperationFactoryProtocol,
        validationApi: TaggedTransactionQueueApiProtocol,
        blockInfoProvider: BlockInfoProviding,
        logger: SDKLoggerProtocol
    ) {
        self.operationFactory = operationFactory
        self.validationApi = validationApi
        self.blockInfoProvider = blockInfoProvider
        self.logger = logger
    }

    public func validate(
        call: any RuntimeCallable,
        origin: ExtrinsicOriginDefining,
        chainId: ChainId
    ) async throws -> SlotReplacementValidity {
        let validity: TransactionValidity
        do {
            validity = try await dryRun(call: call, origin: origin, chainId: chainId)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            logger.warning("Slot replacement dry run failed on \(chainId): \(error)")
            return .inconclusive
        }

        switch validity {
        case .valid:
            return .valid
        case let .invalid(reason):
            logger.debug("Slot replacement invalid on \(chainId): \(reason)")
            return .invalid(reason)
        case let .unknown(reason):
            logger.warning("Slot replacement validity unknown on \(chainId): \(reason)")
            return .inconclusive
        }
    }
}

private extension SlotReplacementValidator {
    func dryRun(
        call: any RuntimeCallable,
        origin: ExtrinsicOriginDefining,
        chainId: ChainId
    ) async throws -> TransactionValidity {
        let built = try await operationFactory.buildExtrinsic(
            { builder in try builder.adding(call: call) },
            origin: origin,
            payingIn: nil
        )
        .asyncExecute()

        // The built hex carries the compact length prefix; the runtime API takes the bare extrinsic.
        let lengthPrefixed = try built.extrinsic.fromHex()
        let extrinsic = try Data(scaleDecoder: ScaleDecoder(data: lengthPrefixed))

        let blockHash = try await blockInfoProvider.fetchCurrentHash()

        return try await validationApi.validateTransaction(
            chainId: chainId,
            source: .external,
            extrinsic: extrinsic,
            at: blockHash
        )
    }
}
