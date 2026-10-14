@testable import polkadot_app
import Foundation
import Testing

final class MessageTimestampFormatterTests {
    private var formatter: MessageTimestampFormatter!
    private let now = Date()

    init() {
        formatter = MessageTimestampFormatter()
    }

    @Test("Returns 'Now' for current time")
    func nowForCurrentTime() {
        let result = formatter.string(for: now, now: now)
        #expect(result == "Now")
    }

    @Test("Returns 'Now' for 30 seconds ago")
    func nowFor30SecondsAgo() {
        let date = now.addingTimeInterval(-30)
        let result = formatter.string(for: date, now: now)
        #expect(result == "Now")
    }

    @Test("Returns 'Now' for future timestamps (clock skew)")
    func nowForFutureTimestamp() {
        let date = now.addingTimeInterval(60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "Now")
    }

    @Test("Returns '1m' for exactly 1 minute ago")
    func oneMinuteAgo() {
        let date = now.addingTimeInterval(-60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "1m")
    }

    @Test("Returns '5m' for 5 minutes ago")
    func fiveMinutesAgo() {
        let date = now.addingTimeInterval(-5 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "5m")
    }

    @Test("Returns '59m' for 59 minutes ago")
    func fiftyNineMinutesAgo() {
        let date = now.addingTimeInterval(-59 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "59m")
    }

    @Test("Returns time format (H:mm) for message older than 1 hour")
    func olderThanOneHour() {
        let date = now.addingTimeInterval(-2 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        let timePattern = #/^\d{1,2}:\d{2}$/#
        #expect(result.contains(timePattern), "Expected H:mm format, got: \(result)")
    }

    @Test("Does not include AM/PM")
    func noAmPm() {
        let date = now.addingTimeInterval(-2 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(!result.contains("AM") && !result.contains("PM"), "Should not contain AM/PM, got: \(result)")
    }

    @Test("Does not have leading zero on hour (except midnight)")
    func noLeadingZeroOnHour() {
        let date = now.addingTimeInterval(-3 * 60 * 60)
        let result = formatter.string(for: date, now: now)

        if result.hasPrefix("0") {
            #expect(result.hasPrefix("0:"), "Leading zero should only appear at midnight (0:mm), got: \(result)")
        }
    }

    @Test("Time format uses colon separator")
    func colonSeparator() {
        let date = now.addingTimeInterval(-2 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result.contains(":"), "Time should use colon separator, got: \(result)")
    }
}
