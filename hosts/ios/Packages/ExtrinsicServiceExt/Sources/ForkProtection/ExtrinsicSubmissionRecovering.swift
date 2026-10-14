import Foundation
import ExtrinsicService

public protocol ExtrinsicSubmissionRecovering {
    func recover(
        builtExtrinsic: ExtrinsicBuiltModel,
        failure: ExtrinsicSubmissionFailure
    ) async -> ExtrinsicSubmissionFailureRecovery
}
