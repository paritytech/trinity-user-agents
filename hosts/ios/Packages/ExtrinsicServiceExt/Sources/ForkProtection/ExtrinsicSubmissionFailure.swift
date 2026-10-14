import Foundation

public enum ExtrinsicSubmissionFailure {
    case preSubmissionValidation
    case submission(Error)
    case txInvalidation
}
