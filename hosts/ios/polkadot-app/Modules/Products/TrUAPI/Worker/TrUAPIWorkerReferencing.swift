import TrUAPIHost

/// The demand held by a single Chat or Pocket consumer.
protocol TrUAPIWorkerClaiming: Sendable {
    func release()
}

extension NativeWorkerClaim: TrUAPIWorkerClaiming {}
