import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing

struct ManualClockTests {
    /// A wait nothing satisfies must give way to `withTimeout`, so a test whose
    /// sleep never comes fails instead of hanging the run.
    @Test
    func aTimeoutEndsAWaitForASleepThatNeverComes() async {
        let clock = ManualClock()

        await #expect(throws: TimeoutError.self) {
            try await withTimeout(.milliseconds(50)) { await clock.waitForSleep(for: .seconds(1)) }
        }
    }
}
