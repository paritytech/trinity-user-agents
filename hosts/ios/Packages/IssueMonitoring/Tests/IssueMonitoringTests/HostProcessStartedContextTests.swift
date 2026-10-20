import Foundation
import Testing
@testable import IssueMonitoring

struct HostProcessStartedContextTests {
    @Test("Process-start context preserves correlation and native numeric types")
    func contextValues() {
        let launchID = UUID(uuidString: "12345678-1234-1234-1234-1234567890ab")!
        let context = HostProcessStartedContext(
            appLaunchID: launchID,
            monotonicMilliseconds: 42.5,
            appVersion: "1.2.3",
            appBuild: "456",
            appVariant: "nightly",
            platform: "ios",
            osVersion: "18.1",
            truapiHostVersion: "0.7.0"
        )

        #expect(context.eventName == "host.process.started")
        #expect(context.attributes["app.launch_id"] as? String == launchID.uuidString.lowercased())
        #expect(context.attributes["event.monotonic_ms"] as? Double == 42.5)
        #expect(context.attributes["host.native_app_version"] as? String == "1.2.3")
        #expect(context.attributes["host.app_build"] as? String == "456")
        #expect(context.attributes["host.app_variant"] as? String == "nightly")
        #expect(context.attributes["host.platform"] as? String == "ios")
        #expect(context.attributes["host.os_version"] as? String == "18.1")
        #expect(context.attributes["truapi.host_version"] as? String == "0.7.0")
    }

    @Test("Optional metadata is omitted instead of inventing placeholder values")
    func optionalValues() {
        let context = HostProcessStartedContext(
            appLaunchID: UUID(),
            monotonicMilliseconds: 0,
            appVersion: nil,
            appBuild: nil,
            appVariant: "debug",
            platform: "ios",
            osVersion: nil,
            truapiHostVersion: nil
        )

        #expect(context.attributes["host.native_app_version"] == nil)
        #expect(context.attributes["host.app_build"] == nil)
        #expect(context.attributes["host.os_version"] == nil)
        #expect(context.attributes["truapi.host_version"] == nil)
    }
}
