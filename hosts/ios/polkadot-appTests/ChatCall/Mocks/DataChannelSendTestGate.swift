actor DataChannelSendTestGate {
    private var isOpen = false
    private var continuations: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        guard !isOpen else { return }
        await withCheckedContinuation { continuations.append($0) }
    }

    func open() {
        isOpen = true
        let continuations = continuations
        self.continuations.removeAll()
        continuations.forEach { $0.resume() }
    }
}
