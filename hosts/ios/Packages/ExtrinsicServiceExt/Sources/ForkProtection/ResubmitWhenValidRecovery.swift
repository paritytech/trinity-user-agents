import Foundation
import SubstrateSdk
import ExtrinsicService
import SubstrateOperation
import ChainStore
import SDKLogger

public final class ResubmitWhenValidRecovery: ExtrinsicSubmissionRecovering {
    static let maxConsecutiveValidationFailures = 10

    private let chainId: ChainId
    private let maxAttempts: Int?
    private let validationApi: TaggedTransactionQueueApiProtocol
    private let blockInfoProvider: BlockInfoProviding
    private let logger: SDKLoggerProtocol?

    public init(
        chainId: ChainId,
        maxAttempts: Int?,
        validationApi: TaggedTransactionQueueApiProtocol,
        blockInfoProvider: BlockInfoProviding,
        logger: SDKLoggerProtocol?
    ) {
        self.chainId = chainId
        self.maxAttempts = maxAttempts
        self.validationApi = validationApi
        self.blockInfoProvider = blockInfoProvider
        self.logger = logger
    }

    public func recover(
        builtExtrinsic: ExtrinsicBuiltModel,
        failure: ExtrinsicSubmissionFailure
    ) async -> ExtrinsicSubmissionFailureRecovery {
        logger?.debug("Submission failed on \(chainId) (\(failure)) — starting validity-driven retry")

        guard let body = try? builtExtrinsic.scaleBody else {
            logger?.error("Extrinsic body is undecodable on \(chainId) — aborting recovery")
            return .abort
        }

        var attempt = 0
        var consecutiveFailures = 0

        do {
            for try await _ in blockInfoProvider.subscribeNewHeads() {
                let outcome = await decideRecovery(
                    builtExtrinsic: builtExtrinsic,
                    body: body,
                    attempt: attempt,
                    consecutiveFailures: consecutiveFailures
                )

                if let decision = outcome.decision {
                    return decision
                }

                consecutiveFailures = outcome.validationFailed ? consecutiveFailures + 1 : 0
                attempt += 1
            }
        } catch {
            logger?.warning("New-heads stream ended on \(chainId): \(error) — aborting recovery")
        }

        return .abort
    }
}

private extension ResubmitWhenValidRecovery {
    struct RecoveryOutcome {
        let decision: ExtrinsicSubmissionFailureRecovery?
        let validationFailed: Bool

        static let waitForNextBlock = RecoveryOutcome(decision: nil, validationFailed: false)
        static let validationErrored = RecoveryOutcome(decision: nil, validationFailed: true)

        static func decided(_ decision: ExtrinsicSubmissionFailureRecovery) -> RecoveryOutcome {
            RecoveryOutcome(decision: decision, validationFailed: false)
        }
    }

    func decideRecovery(
        builtExtrinsic: ExtrinsicBuiltModel,
        body: Data,
        attempt: Int,
        consecutiveFailures: Int
    ) async -> RecoveryOutcome {
        if let maxAttempts, attempt >= maxAttempts {
            logger?.info("Reached maxAttempts (\(maxAttempts)) on \(chainId) — aborting")
            return .decided(.abort)
        }

        if consecutiveFailures >= Self.maxConsecutiveValidationFailures {
            logger?.error(
                "Validation failed \(consecutiveFailures) times in a row on \(chainId) — aborting"
            )
            return .decided(.abort)
        }

        do {
            let blockHash = try await blockInfoProvider.fetchCurrentHash()
            let validity = try await validationApi.validateTransaction(
                chainId: chainId,
                source: .external,
                extrinsic: body,
                at: blockHash
            )

            switch validity {
            case .valid:
                logger?.info("Extrinsic valid again on \(chainId) — resubmitting")
                return .decided(.resubmit(builtExtrinsic))
            case let .invalid(reason):
                if validity.isMortalityExpired {
                    logger?.info("Mortality expired on \(chainId) (\(reason)) — aborting")
                    return .decided(.abort)
                }
                logger?.debug("Still invalid on \(chainId) (\(reason)) — waiting for next block")
                return .waitForNextBlock
            case let .unknown(reason):
                logger?.debug("Validity unresolved on \(chainId) (\(reason)) — waiting for next block")
                return .waitForNextBlock
            }
        } catch {
            logger?.debug("Validation failed on \(chainId): \(error) — waiting for next block")
            return .validationErrored
        }
    }
}
