import Foundation
import SubstrateSdk
import Operation_iOS

public final class WalletNoDelayExecVerifier: WalletDelayedExecVerifing {
    public func executesCallWithDelay(
        _: MetaAccountModelProtocol,
        chain _: ChainProtocol
    ) -> Bool {
        false
    }

    public init() {}
}

public final class WalletNoDelayExecutionProvider {
    let delayedExecVerifier = WalletNoDelayExecVerifier()

    public init() {}
}

extension WalletNoDelayExecutionProvider: WalletDelayedExecutionProviding {
    public func setup() {}

    public func throttle() {}

    public func subscribeDelayedExecVerifier(
        _: AnyObject,
        notifyingIn queue: DispatchQueue,
        onChange: @escaping (WalletDelayedExecVerifing) -> Void
    ) {
        dispatchInQueueWhenPossible(queue) { [delayedExecVerifier] in
            onChange(delayedExecVerifier)
        }
    }

    public func unsubscribe(_: AnyObject) {}

    public func getCurrentState() -> WalletDelayedExecVerifing {
        delayedExecVerifier
    }
}
