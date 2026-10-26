import Foundation

public struct NativeHostTelemetry: Encodable, Sendable {
    public let hostSessionId: String
    public let appVersion: String?
    public let appBuild: String?
    public let appVariant: String
    public let platform: String
    public let osVersion: String
    public let truapiHostVersion: String
    public let schemaVersion: String

    public init(hostSessionID: UUID, origin: HostEventOrigin, truapiHostVersion: String) {
        hostSessionId = hostSessionID.uuidString.lowercased()
        appVersion = origin.appVersion
        appBuild = origin.appBuild.map(String.init)
        appVariant = origin.appVariant
        platform = "ios"
        osVersion = origin.osVersion
        self.truapiHostVersion = truapiHostVersion
        schemaVersion = "1"
    }
}
