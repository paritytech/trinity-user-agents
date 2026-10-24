import Foundation

/// The fields every host event carries to say which install and which launch sent it.
public struct HostEventOrigin: Sendable {
    public let launchID: UUID
    public let osVersion: String
    public let appVersion: String?
    public let appBuild: Int?

    public init(launchID: UUID, osVersion: String, appVersion: String?, appBuild: Int?) {
        self.launchID = launchID
        self.osVersion = osVersion
        self.appVersion = appVersion
        self.appBuild = appBuild
    }

    public static let current = HostEventOrigin(
        launchID: UUID(),
        osVersion: plainOSVersion(ProcessInfo.processInfo.operatingSystemVersion),
        appVersion: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String,
        appBuild: (Bundle.main.infoDictionary?["CFBundleVersion"] as? String).flatMap { Int($0) }
    )

    var tags: [String: String] {
        var tags = [
            "host.platform": "ios",
            "host.os_version": osVersion
        ]
        tags["host.native_app_version"] = appVersion
        return tags
    }

    var extras: [String: Any] {
        var extras: [String: Any] = ["app.launch_id": launchID.uuidString.lowercased()]
        extras["host.app_build"] = appBuild
        return extras
    }

    static func plainOSVersion(_ version: OperatingSystemVersion) -> String {
        let baseVersion = "\(version.majorVersion).\(version.minorVersion)"
        return version.patchVersion == 0 ? baseVersion : "\(baseVersion).\(version.patchVersion)"
    }
}
