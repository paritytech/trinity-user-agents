@testable import polkadot_app
import Foundation

final class CameraPermissionServiceSpy: CameraPermissionServicing, @unchecked Sendable {
    private let status: CameraPermissionResult
    private let promptResult: CameraPermissionResult
    private let lock = NSLock()
    private var count = 0

    init(status: CameraPermissionResult, promptResult: CameraPermissionResult = .denied) {
        self.status = status
        self.promptResult = promptResult
    }

    var requestCount: Int {
        lock.withLock { count }
    }

    func permission() -> CameraPermissionResult {
        status
    }

    func requestPermission() async -> CameraPermissionResult {
        lock.withLock { count += 1 }
        return promptResult
    }
}
