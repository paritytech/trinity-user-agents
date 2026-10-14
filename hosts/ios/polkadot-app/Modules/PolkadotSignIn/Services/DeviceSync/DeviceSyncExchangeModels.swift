import Foundation

typealias DeviceSyncSleep = @Sendable (Duration) async throws -> Void

let liveDeviceSyncSleep: DeviceSyncSleep = { duration in
    try await Task.sleep(for: duration)
}

enum DeviceSyncConnectionFailure: Error {
    case connectionFailed(Error)
    case disconnected
    case collectionFailed(Error)
    case sendFailed(Error)
}
