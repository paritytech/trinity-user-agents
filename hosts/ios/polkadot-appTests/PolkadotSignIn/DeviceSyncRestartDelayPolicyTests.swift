@testable import polkadot_app
import Testing

struct DeviceSyncRestartDelayPolicyTests {
    @Test("Backoff doubles from one second and caps at thirty seconds")
    func delaySequence() {
        let policy = DeviceSyncRestartDelayPolicy()

        #expect(policy.delay(forAttempt: 0) == .seconds(1))
        #expect(policy.delay(forAttempt: 1) == .seconds(1))
        #expect(policy.delay(forAttempt: 2) == .seconds(2))
        #expect(policy.delay(forAttempt: 3) == .seconds(4))
        #expect(policy.delay(forAttempt: 4) == .seconds(8))
        #expect(policy.delay(forAttempt: 5) == .seconds(16))
        #expect(policy.delay(forAttempt: 6) == .seconds(30))
        #expect(policy.delay(forAttempt: 12) == .seconds(30))
    }
}
