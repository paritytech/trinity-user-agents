import Foundation

/// Process-wide metadata shared by native host telemetry events.
public struct HostEventOrigin: Sendable {
    public let launchID: UUID
    public let osVersion: String
    public let appVersion: String?
    public let appBuild: Int?
    public let appVariant: String

    public init(
        launchID: UUID,
        osVersion: String,
        appVersion: String?,
        appBuild: Int?,
        appVariant: String
    ) {
        self.launchID = launchID
        self.osVersion = osVersion
        self.appVersion = appVersion
        self.appBuild = appBuild
        self.appVariant = appVariant
    }

    public static let current = HostEventOrigin(
        launchID: UUID(),
        osVersion: plainOSVersion(ProcessInfo.processInfo.operatingSystemVersion),
        appVersion: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String,
        appBuild: (Bundle.main.infoDictionary?["CFBundleVersion"] as? String).flatMap { Int($0) },
        appVariant: currentAppVariant
    )
}

private extension HostEventOrigin {
    static func plainOSVersion(_ version: OperatingSystemVersion) -> String {
        let baseVersion = "\(version.majorVersion).\(version.minorVersion)"
        return version.patchVersion == 0 ? baseVersion : "\(baseVersion).\(version.patchVersion)"
    }

    static var currentAppVariant: String {
        let bundleIdentifier = Bundle.main.bundleIdentifier

        if bundleIdentifier?.hasSuffix(".safety") == true {
            return "safetynet"
        }

        if bundleIdentifier?.hasSuffix(".develop") == true {
            return "debug"
        }

        return IssueMonitoringFactory.isReportingSupported ? "nightly" : "release"
    }
}
