import Foundation
import ExtrinsicService
import SubstrateSdk
import KeyDerivation
import IssueMonitoring

public enum SlotSubmissionError: Error {
    case extrinsicFailed(Error)
}

extension SlotSubmissionError: ReportableError {
    public var reportCode: String {
        switch self {
        case let .extrinsicFailed(underlying):
            "extrinsicFailed.\(String(reflecting: type(of: underlying)))"
        }
    }
}

public protocol SlotAssignmentSubmitting {
    func submit(
        call: any RuntimeCallable,
        makeOrigin: @Sendable (SubstrateSdk.ChainId) async throws -> ExtrinsicOriginDefining,
        chainId: SubstrateSdk.ChainId
    ) async throws
}

public final class SlotAssignmentSubmitter: SlotAssignmentSubmitting {
    private let monitorFactory: any ExtrinsicSubmitMonitorFactoryProtocol

    public init(
        monitorFactory: any ExtrinsicSubmitMonitorFactoryProtocol
    ) {
        self.monitorFactory = monitorFactory
    }

    public func submit(
        call: any RuntimeCallable,
        makeOrigin: @Sendable (SubstrateSdk.ChainId) async throws -> ExtrinsicOriginDefining,
        chainId: SubstrateSdk.ChainId
    ) async throws {
        let origin = try await makeOrigin(chainId)

        let submission = try await monitorFactory.submitAndMonitorWrapper(
            extrinsicBuilderClosure: { builder in
                try builder.adding(call: call)
            },
            origin: origin,
            params: ExtrinsicSubmissionParams(feeAssetId: nil, eventsMatcher: nil)
        )
        .asyncExecute()

        switch submission.status {
        case .success:
            return
        case let .failure(failedExtrinsic):
            throw SlotSubmissionError.extrinsicFailed(failedExtrinsic.error)
        }
    }
}
