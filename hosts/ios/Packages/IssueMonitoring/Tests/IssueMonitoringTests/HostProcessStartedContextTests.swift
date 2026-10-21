import Foundation
import Testing
@testable import IssueMonitoring

struct HostProcessStartedContextTests {
    @Test("Process-start context preserves correlation and native numeric types")
    func contextValues() {
        let launchID = UUID(uuidString: "12345678-1234-1234-1234-1234567890ab")!
        let context = HostProcessStartedContext(
            appLaunchID: launchID,
            processStartUptimeMilliseconds: 42.5,
            appVersion: "1.2.3",
            appBuild: 456,
            appVariant: "nightly",
            platform: "ios",
            osVersion: "18.1",
            truapiHostVersion: "0.7.0"
        )

        #expect(context.eventName == "host.process.started")
        #expect(context.sentryFields.extras["app.launch_id"] as? String == launchID.uuidString.lowercased())
        #expect(context.sentryFields.extras["event.monotonic_ms"] as? Double == 42.5)
        #expect(context.sentryFields.tags["host.native_app_version"] == "1.2.3")
        #expect(context.sentryFields.extras["host.app_build"] as? Int == 456)
        #expect(context.sentryFields.tags["host.app_variant"] == "nightly")
        #expect(context.sentryFields.tags["host.platform"] == "ios")
        #expect(context.sentryFields.tags["host.os_version"] == "18.1")
        #expect(context.sentryFields.tags["truapi.host_version"] == "0.7.0")
    }

    @Test("Optional metadata is omitted instead of inventing placeholder values")
    func optionalValues() {
        let context = HostProcessStartedContext(
            appLaunchID: UUID(),
            processStartUptimeMilliseconds: 0,
            appVersion: nil,
            appBuild: nil,
            appVariant: "debug",
            platform: "ios",
            osVersion: nil,
            truapiHostVersion: nil
        )

        #expect(context.sentryFields.tags["host.native_app_version"] == nil)
        #expect(context.sentryFields.extras["host.app_build"] == nil)
        #expect(context.sentryFields.tags["host.os_version"] == nil)
        #expect(context.sentryFields.tags["truapi.host_version"] == nil)
    }
}
