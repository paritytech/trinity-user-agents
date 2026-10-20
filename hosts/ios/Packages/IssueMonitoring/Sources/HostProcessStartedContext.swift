import Foundation

public struct HostProcessStartedContext: Sendable {
    public let appLaunchID: UUID
    public let monotonicMilliseconds: Double
    public let appVersion: String?
    public let appBuild: String?
    public let appVariant: String
    public let platform: String
    public let osVersion: String?
    public let truapiHostVersion: String?

    public init(
        appLaunchID: UUID,
        monotonicMilliseconds: Double,
        appVersion: String?,
        appBuild: String?,
        appVariant: String,
        platform: String,
        osVersion: String?,
        truapiHostVersion: String?
    ) {
        self.appLaunchID = appLaunchID
        self.monotonicMilliseconds = monotonicMilliseconds
        self.appVersion = appVersion
        self.appBuild = appBuild
        self.appVariant = appVariant
        self.platform = platform
        self.osVersion = osVersion
        self.truapiHostVersion = truapiHostVersion
    }

    let eventName = "host.process.started"

    var attributes: [String: Any] {
        var values: [String: Any] = [
            "app.launch_id": appLaunchID.uuidString.lowercased(),
            "event.monotonic_ms": monotonicMilliseconds,
            "host.app_variant": appVariant,
            "host.platform": platform
        ]

        values["host.native_app_version"] = appVersion
        values["host.app_build"] = appBuild
        values["host.os_version"] = osVersion
        values["host.truapi_host_version"] = truapiHostVersion

        return values
    }
}
