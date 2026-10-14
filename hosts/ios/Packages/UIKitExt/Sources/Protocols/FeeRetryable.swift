import Foundation

@MainActor
public protocol FeeRetryable {
    func presentFeeStatus(
        on view: ControllerBackedProtocol?,
        locale: Locale?,
        retryAction: @escaping () -> Void
    )
}
