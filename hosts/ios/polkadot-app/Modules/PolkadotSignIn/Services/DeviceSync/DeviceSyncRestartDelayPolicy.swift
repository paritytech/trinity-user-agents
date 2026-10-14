import Foundation

struct DeviceSyncRestartDelayPolicy {
    let maxDelaySeconds: Int = 30

    func delay(forAttempt attempt: Int) -> Duration {
        let exponent = min(max(attempt - 1, 0), 5)
        let seconds = min(1 << exponent, maxDelaySeconds)
        return .seconds(seconds)
    }
}
