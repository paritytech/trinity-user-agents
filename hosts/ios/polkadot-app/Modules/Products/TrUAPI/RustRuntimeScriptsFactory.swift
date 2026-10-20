import Foundation
import Products
import TrUAPIHost

struct NativeHostTelemetry: Encodable {
    let hostSessionId: String
    let appVersion: String?
    let appBuild: String?
    let appVariant: String
    let platform: String
    let osVersion: String
    let truapiHostVersion: String
    let schemaVersion: String

    static func current(hostSessionId: UUID) -> NativeHostTelemetry {
        let hostVersion = truapiHostVersion()

        return NativeHostTelemetry(
            hostSessionId: hostSessionId.uuidString.lowercased(),
            appVersion: Bundle.main.appVersion,
            appBuild: Bundle.main.appBuild,
            appVariant: currentAppVariant,
            platform: "ios",
            osVersion: ProcessInfo.processInfo.operatingSystemVersionString,
            truapiHostVersion: hostVersion,
            schemaVersion: "1"
        )
    }

    private static var currentAppVariant: String {
        #if NIGHTLY
            "nightly"
        #elseif SAFETYNET
            "safetynet"
        #elseif UNSTABLE
            "debug"
        #elseif DEBUG
            "debug"
        #else
            "release"
        #endif
    }
}

/// Bootstrap must publish the bridge endpoint before the container runs.
final class RustRuntimeScriptsFactory {
    private let bootstrapScript: String
    private let hostTelemetry: NativeHostTelemetry?

    init(bootstrapScript: String, hostTelemetry: NativeHostTelemetry? = nil) {
        self.bootstrapScript = bootstrapScript
        self.hostTelemetry = hostTelemetry
    }

    func makeScripts() throws -> [JSEngineScript] {
        var scripts: [JSEngineScript] = []

        if let hostTelemetry {
            scripts.append(try makeHostTelemetryScript(hostTelemetry))
        }

        scripts.append(contentsOf: try [
            JSEngineScript(content: bootstrapScript, insertionPoint: .atDocStart),
            JSEngineScript(
                content: ContainerScriptBundle.load(),
                insertionPoint: .atDocStart,
                frameScope: .allFrames
            )
        ])

        return scripts
    }

    private func makeHostTelemetryScript(_ telemetry: NativeHostTelemetry) throws -> JSEngineScript {
        let data = try JSONEncoder().encode(telemetry)
        let json = String(decoding: data, as: UTF8.self)
        let script = """
        Object.defineProperty(globalThis, "__trinityHostTelemetry", {
            value: Object.freeze(\(json)),
            writable: false,
            configurable: false
        });
        """

        return JSEngineScript(content: script, insertionPoint: .atDocStart)
    }
}
