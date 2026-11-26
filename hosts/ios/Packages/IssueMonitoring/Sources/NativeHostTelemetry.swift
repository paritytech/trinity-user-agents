import Foundation

/// The schema-v1 object a product page reads from `globalThis.__trinityHostTelemetry`.
public struct NativeHostTelemetry: Encodable, Sendable {
    public let hostSessionId: String
    public let appVersion: String?
    public let appBuild: String?
    public let appVariant: String
    public let platform: String
    public let osVersion: String
    public let truapiHostVersion: String
    public let schemaVersion: String
    /// Set when a tap opened the page; optional and additive, so the schema version stays 1.
    public let openedAtEpochMs: Int?
    public let openSource: String?

    public init(
        hostSessionID: UUID,
        origin: HostEventOrigin,
        truapiHostVersion: String,
        openTap: HostOpenTap? = nil
    ) {
        hostSessionId = hostSessionID.uuidString.lowercased()
        appVersion = origin.appVersion
        appBuild = origin.appBuild.map(String.init)
        appVariant = origin.appVariant
        platform = "ios"
        osVersion = origin.osVersion
        self.truapiHostVersion = truapiHostVersion
        schemaVersion = "1"
        openedAtEpochMs = openTap?.openedAtEpochMilliseconds
        openSource = openTap?.source.rawValue
    }
}
