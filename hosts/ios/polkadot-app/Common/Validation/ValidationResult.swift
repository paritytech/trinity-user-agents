import Foundation
import UIKitExt

enum CommonValidationIssueContext {
    case insufficientBalance(String?)
    case calculatingFee

    var additionalInfo: String? {
        switch self {
        case let .insufficientBalance(info):
            info
        case .calculatingFee:
            nil
        }
    }
}

enum ValidationResult {
    case issue(message: String, context: Any?)
    case valid

    var isValid: Bool {
        switch self {
        case .valid:
            true
        default:
            false
        }
    }
}

@MainActor
protocol ValidationResultPresentable {
    func didReceiveValidation(result: ValidationResult)
}

typealias ControllerValidationResultPresentable = ControllerBackedProtocol & ValidationResultPresentable
