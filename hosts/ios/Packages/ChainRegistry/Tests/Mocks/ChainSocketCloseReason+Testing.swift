import ChainRegistry

extension ChainSocketCloseReason {
    var isPeerClosed: Bool {
        if case .peerClosed = self { return true }
        return false
    }

    var isTransport: Bool {
        if case .transport = self { return true }
        return false
    }

    var isLivenessTimeout: Bool {
        if case .livenessTimeout = self { return true }
        return false
    }

    var isConnectTimeout: Bool {
        if case .connectTimeout = self { return true }
        return false
    }

    var isResumed: Bool {
        if case .resumed = self { return true }
        return false
    }
}
