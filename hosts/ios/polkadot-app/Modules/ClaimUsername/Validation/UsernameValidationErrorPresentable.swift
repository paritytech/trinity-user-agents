import Foundation

@MainActor
protocol UsernameValidationErrorPresentable: ValidationErrorPresentable {
    func presentMinLengthViolation(
        from view: ControllerValidationResultPresentable,
        requiredMinLength: Int
    )

    func presentMaxLengthViolation(
        from view: ControllerValidationResultPresentable,
        requiredMaxLength: Int
    )

    func presentUsernameTaken(
        from view: ControllerValidationResultPresentable
    )

    func presentUsernameInvalid(from view: ControllerValidationResultPresentable)
}

extension UsernameValidationErrorPresentable {
    func presentMinLengthViolation(
        from view: ControllerValidationResultPresentable,
        requiredMinLength: Int
    ) {
        let length = Int32(requiredMinLength)
        presentIssue(
            with: String(localized: .claimUsernameMinLengthError(length)),
            on: view
        )
    }

    func presentMaxLengthViolation(
        from view: ControllerValidationResultPresentable,
        requiredMaxLength: Int
    ) {
        let length = Int32(requiredMaxLength)
        presentIssue(
            with: String(localized: .claimUsernameMaxLengthError(length)),
            on: view
        )
    }

    func presentUsernameTaken(
        from view: ControllerValidationResultPresentable
    ) {
        view.didReceiveValidation(
            result: .issue(
                message: String(localized: .claimUsernameIsTaken),
                context: UsernameValidationContext.usernameTaken
            )
        )
    }

    func presentUsernameInvalid(from view: ControllerValidationResultPresentable) {
        view.didReceiveValidation(
            result: .issue(
                message: String(localized: .claimUsernameInvalid),
                context: UsernameValidationContext.usernameInvalid
            )
        )
    }
}
