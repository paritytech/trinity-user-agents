import Foundation

@MainActor
public protocol CommonRetryable {
    // swiftlint:disable:next function_parameter_count
    func presentRequestStatus(
        on view: ControllerBackedProtocol?,
        title: String,
        message: String,
        cancelAction: String,
        locale: Locale?,
        retryAction: @escaping () -> Void
    )

    func presentTryAgainOperation(
        on view: ControllerBackedProtocol?,
        title: String,
        message: String,
        actionTitle: String,
        retryAction: @escaping () -> Void
    )
}
