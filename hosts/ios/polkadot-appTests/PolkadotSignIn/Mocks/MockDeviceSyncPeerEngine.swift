@testable import polkadot_app

actor MockDeviceSyncPeerEngine: DeviceSyncPeerEngining {
    private let disposalGate: DeviceSyncCancellationGate?
    private var started = false
    private var startWaiters = [CheckedContinuation<Void, Never>]()
    private var disposalStarted = false
    private var disposalWaiters = [CheckedContinuation<Void, Never>]()
    private(set) var isDisposed = false

    init(disposalGate: DeviceSyncCancellationGate? = nil) {
        self.disposalGate = disposalGate
    }

    func start() async {
        started = true
        let startWaiters = startWaiters
        self.startWaiters.removeAll()
        startWaiters.forEach { $0.resume() }
    }

    func dispose() async {
        disposalStarted = true
        let disposalWaiters = disposalWaiters
        self.disposalWaiters.removeAll()
        disposalWaiters.forEach { $0.resume() }

        await disposalGate?.wait()
        isDisposed = true
    }

    func waitUntilDisposalStarts() async {
        guard !disposalStarted else { return }
        await withCheckedContinuation { disposalWaiters.append($0) }
    }

    func waitUntilStarted() async {
        guard !started else { return }
        await withCheckedContinuation { startWaiters.append($0) }
    }
}
