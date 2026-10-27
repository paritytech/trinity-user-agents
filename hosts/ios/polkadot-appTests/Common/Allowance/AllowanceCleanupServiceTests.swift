import Foundation
import Individuality
import Testing
@testable import polkadot_app

struct AllowanceCleanupServiceTests {
    @Test func failingTargetDoesNotStopTheOthers() async {
        let failing = MockAllowanceManager()
        failing.releaseError = CleanupTestError()
        let healthy = MockAllowanceManager()

        await AllowanceCleanupService(
            targets: [
                .init(manager: failing, strategy: .accountIds([])),
                .init(manager: healthy, strategy: .accountIds([]))
            ],
            logger: MockLogger()
        ).cleanup()

        #expect(failing.releaseCallCount == 1)
        #expect(healthy.releaseCallCount == 1)
    }
}

private struct CleanupTestError: Error {}
