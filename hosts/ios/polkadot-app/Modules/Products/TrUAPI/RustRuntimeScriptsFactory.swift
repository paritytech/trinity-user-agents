import Foundation
import Products
import TrUAPIHost
import UIKit

private func currentTrUAPIHostVersion() -> String {
    truapiHostVersion()
}

struct NativeHostTelemetry: Encodable {
    let hostSessionId: String
    let appVersion: String?
    let appBuild: String?
    let appVariant: String
    let platform: String
    let osVersion: String
    let truapiHostVersion: String
    let schemaVersion: String

    static func current(hostSessionId: UUID) -> NativeHostTelemetry? {
        #if NIGHTLY || SAFETYNET || UNSTABLE || DEBUG
            current(hostSessionId: hostSessionId, enabled: true)
        #else
            current(hostSessionId: hostSessionId, enabled: false)
        #endif
    }

    static func current(hostSessionId: UUID, enabled: Bool) -> NativeHostTelemetry? {
        guard enabled else { return nil }

        return NativeHostTelemetry(
            hostSessionId: hostSessionId.uuidString.lowercased(),
            appVersion: Bundle.main.appVersion,
            appBuild: Bundle.main.appBuild,
            appVariant: currentAppVariant,
            platform: "ios",
            osVersion: UIDevice.current.systemVersion,
            truapiHostVersion: currentTrUAPIHostVersion(),
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
    private let logger: LoggerProtocol

    init(
        bootstrapScript: String,
        hostTelemetry: NativeHostTelemetry? = nil,
        logger: LoggerProtocol
    ) {
        self.bootstrapScript = bootstrapScript
        self.hostTelemetry = hostTelemetry
        self.logger = logger
    }

    func makeScripts() throws -> [JSEngineScript] {
        var scripts: [JSEngineScript] = []

        if let hostTelemetry {
            do {
                scripts.append(try makeHostTelemetryScript(hostTelemetry))
            } catch {
                logger.warning("[truapi] host telemetry bootstrap could not be encoded: \(error)")
            }
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
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(telemetry)
        guard let json = String(data: data, encoding: .utf8) else {
            throw CocoaError(.fileReadInapplicableStringEncoding)
        }
        let script = """
        try {
            Object.defineProperty(globalThis, "__trinityHostTelemetry", {
                value: Object.freeze(\(json)),
                writable: false,
                configurable: false
            });
        } catch (error) {
            console.warn("Native host telemetry bootstrap was not installed", error);
        }
        """

        return JSEngineScript(content: script, insertionPoint: .atDocStart)
    }
}
