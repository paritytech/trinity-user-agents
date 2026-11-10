@testable import polkadot_app
import Foundation

final class RecordPermissionRequesterSpy: RecordPermissionRequesting, @unchecked Sendable {
    private let grants: Bool
    private let lock = NSLock()
    private var count = 0

    init(grants: Bool) {
        self.grants = grants
    }

    var requestCount: Int {
        lock.withLock { count }
    }

    func requestRecordPermission() async -> Bool {
        lock.withLock { count += 1 }
        return grants
    }
}
