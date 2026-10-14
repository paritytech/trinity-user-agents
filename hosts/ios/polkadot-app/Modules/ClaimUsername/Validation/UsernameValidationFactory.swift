import Foundation
import Foundation_iOS

@MainActor
protocol UsernameValidationFactoryProtocol {
    func notViolatingMinLength(
        for partialUsername: String,
        minLength: Int
    ) -> DataValidating

    func notViolatingMaxLength(
        for partialUsername: String,
        maxLength: Int
    ) -> DataValidating

    func notTaken(from checkResult: UsernameAvailableType?) -> DataValidating
    func notValid(from availableType: UsernameAvailableType?) -> DataValidating
}

@MainActor
class UsernameValidationFactory {
    weak var view: ControllerValidationResultPresentable?

    let presentable: UsernameValidationErrorPresentable

    init(presentable: UsernameValidationErrorPresentable) {
        self.presentable = presentable
    }
}

extension UsernameValidationFactory: UsernameValidationFactoryProtocol {
    func notViolatingMinLength(
        for partialUsername: String,
        minLength: Int
    ) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            self?.presentable.presentMinLengthViolation(
                from: view,
                requiredMinLength: minLength
            )
        }, preservesCondition: {
            partialUsername.count >= minLength
        })
    }

    func notViolatingMaxLength(
        for partialUsername: String,
        maxLength: Int
    ) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            self?.presentable.presentMaxLengthViolation(
                from: view,
                requiredMaxLength: maxLength
            )
        }, preservesCondition: {
            partialUsername.count <= maxLength
        })
    }

    func notTaken(from availableType: UsernameAvailableType?) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            self?.presentable.presentUsernameTaken(from: view)
        }, preservesCondition: {
            guard let availableType else {
                return false
            }
            switch availableType {
            case .taken:
                return false
            default:
                return true
            }
        })
    }

    func notValid(from availableType: UsernameAvailableType?) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            self?.presentable.presentUsernameInvalid(from: view)
        }, preservesCondition: {
            guard let availableType else {
                return false
            }
            switch availableType {
            case .invalid:
                return false
            default:
                return true
            }
        })
    }
}
