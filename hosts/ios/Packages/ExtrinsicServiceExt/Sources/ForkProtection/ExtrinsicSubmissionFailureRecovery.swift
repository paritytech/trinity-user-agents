import Foundation
import ExtrinsicService

public enum ExtrinsicSubmissionFailureRecovery {
    case abort
    case resubmit(ExtrinsicBuiltModel)
}
