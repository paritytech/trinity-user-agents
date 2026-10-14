import Foundation

public protocol WalletDelayedExecutionProviding {
    func setup()
    func throttle()

    func subscribeDelayedExecVerifier(
        _ target: AnyObject,
        notifyingIn queue: DispatchQueue,
        onChange: @escaping (WalletDelayedExecVerifing) -> Void
    )

    func unsubscribe(_ target: AnyObject)

    func getCurrentState() -> WalletDelayedExecVerifing
}
