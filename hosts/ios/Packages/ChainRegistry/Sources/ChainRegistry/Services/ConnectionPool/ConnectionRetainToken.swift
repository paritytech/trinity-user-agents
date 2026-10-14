import Foundation

protocol ConnectionRetaining: AnyObject {
    func releaseRetain(_ retained: ConnectionRetainToken.Retained)
}

/// Keeps connections alive across app backgrounding for as long as the token is held.
///
/// Releases on `deinit` or an explicit ``release()``.
public final class ConnectionRetainToken {
    enum Retained {
        /// Per-connection refcounted retains for a fixed set of connections.
        case connections([(id: ObjectIdentifier, connection: AnyObject)])
        /// A single global "retain everything" whose snapshot is held strongly.
        case all(snapshot: [AnyObject])
    }

    private let lock = NSLock()
    private weak var owner: ConnectionRetaining?
    private var retained: Retained?
    private var isReleased = false

    init(owner: ConnectionRetaining, retained: Retained) {
        self.owner = owner
        self.retained = retained
    }

    public init() {
        owner = nil
        retained = nil
    }

    deinit {
        release()
    }

    public func release() {
        lock.lock()
        let shouldRelease = !isReleased
        isReleased = true
        let owner = owner
        let retained = retained
        self.retained = nil
        lock.unlock()

        guard shouldRelease, let owner, let retained else {
            return
        }

        owner.releaseRetain(retained)
    }
}

extension ConnectionRetainToken: @unchecked Sendable {}
