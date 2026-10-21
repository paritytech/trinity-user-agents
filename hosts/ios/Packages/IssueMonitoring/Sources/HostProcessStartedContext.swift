import Foundation

public struct HostProcessStartedContext: Sendable {
    public let appLaunchID: UUID
    public let processStartUptimeMilliseconds: Double
    public let appVersion: String?
    public let appBuild: Int?
    public let appVariant: String
    public let platform: String
    public let osVersion: String?
    public let truapiHostVersion: String?

    public init(
        appLaunchID: UUID,
        processStartUptimeMilliseconds: Double,
        appVersion: String?,
        appBuild: Int?,
        appVariant: String,
        platform: String,
        osVersion: String?,
        truapiHostVersion: String?
    ) {
        self.appLaunchID = appLaunchID
        self.processStartUptimeMilliseconds = processStartUptimeMilliseconds
        self.appVersion = appVersion
        self.appBuild = appBuild
        self.appVariant = appVariant
        self.platform = platform
        self.osVersion = osVersion
        self.truapiHostVersion = truapiHostVersion
    }

    let eventName = "host.process.started"

    var sentryFields: HostProcessStartedSentryFields {
        var tags = [
            "host.app_variant": appVariant,
            "host.platform": platform
        ]
        var extras: [String: Any] = [
            "app.launch_id": appLaunchID.uuidString.lowercased(),
            "event.monotonic_ms": processStartUptimeMilliseconds
        ]

        tags["host.native_app_version"] = appVersion
        extras["host.app_build"] = appBuild
        tags["host.os_version"] = osVersion
        tags["truapi.host_version"] = truapiHostVersion

        return HostProcessStartedSentryFields(tags: tags, extras: extras)
    }
}

struct HostProcessStartedSentryFields {
    let tags: [String: String]
    let extras: [String: Any]
}
