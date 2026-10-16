import Testing
@testable import Individuality

struct NotificationParametersProviderTests {
    @Test func periodFollowsRuntimeDurationRatherThanDay() throws {
        #expect(try NotificationParametersProvider.period(atSeconds: 7_200, duration: 3_600) == 2)
        #expect(try NotificationParametersProvider.period(atSeconds: 86_399, duration: 86_400) == 0)
    }

    @Test func rejectsZeroDuration() {
        #expect(throws: NotificationParametersError.zeroPeriodDuration) {
            try NotificationParametersProvider.period(atSeconds: 1, duration: 0)
        }
    }
}
