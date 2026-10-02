import Foundation
import Testing
import TrUAPIHost

struct HostBridgeDefaultsTests {
    /// A host that runs no workers still hands each operation its own id: the
    /// core counts demand per id, and a host overriding only `endOperation`
    /// would end every operation at once if they all shared one.
    @Test
    func defaultBeginOperationNamesEachOperationSeparately() async throws {
        let bridge = StubHostBridge()

        let first = try await bridge.beginOperation(productId: "test.dot", label: "funding")
        let second = try await bridge.beginOperation(productId: "test.dot", label: "")

        #expect(first != second)
        #expect(first != 0)
        #expect(second != 0)
    }

    @Test
    func localizedTimestampsRespectDSTAndLocalMidnight() async throws {
        let bridge = StubHostBridge()
        let parser = ISO8601DateFormatter()
        let instants = [
            "2024-03-10T06:59:00Z", "2024-03-10T07:01:00Z",
            "2024-03-10T04:59:00Z", "2024-03-10T05:01:00Z",
            "2024-11-03T05:30:00Z", "2024-11-03T06:30:00Z",
        ].map { UInt64(parser.date(from: $0)!.timeIntervalSince1970 * 1_000) }
        let response = try await bridge.localizeTimestamps(request: .init(
            timestampsMs: instants, languageTag: "en-US", timeZone: "America/New_York"
        ))
        #expect(response.timestamps[2].localDate == "2024-03-09")
        #expect(response.timestamps[3].localDate == "2024-03-10")
        let expected = DateFormatter()
        expected.locale = Locale(identifier: "en-US")
        expected.timeZone = TimeZone(identifier: "America/New_York")
        expected.timeStyle = .short
        #expect(response.timestamps[0].time == expected.string(from: parser.date(from: "2024-03-10T06:59:00Z")!))
        #expect(response.timestamps[1].time == expected.string(from: parser.date(from: "2024-03-10T07:01:00Z")!))
        #expect(response.timestamps[4].time == response.timestamps[5].time)
        #expect(response.timestamps[4].dateTime != response.timestamps[5].dateTime)

        let changed = try await bridge.localizeTimestamps(request: .init(
            timestampsMs: instants, languageTag: "fr-FR", timeZone: "Europe/Paris"
        ))
        #expect(changed.timestamps[2].localDate == "2024-03-10")
        #expect(changed.timestamps[0].date != response.timestamps[0].date)
    }

    @Test
    func localizedDateKeysStayGregorianAndUnknownZonesAreRejected() async throws {
        let bridge = StubHostBridge()
        let response = try await bridge.localizeTimestamps(request: .init(
            timestampsMs: [0], languageTag: "th-TH-u-ca-buddhist", timeZone: "Asia/Bangkok"
        ))
        #expect(response.timestamps[0].localDate == "1970-01-01")
        await #expect(throws: HostRejection.self) {
            try await bridge.localizeTimestamps(request: .init(
                timestampsMs: [0], languageTag: "en-US", timeZone: "Not/AZone"
            ))
        }
    }
}
