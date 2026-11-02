import Foundation
import IssueMonitoring
import Products
import TrUAPIHost

/// Bootstrap must publish the bridge endpoint before the container runs.
final class RustRuntimeScriptsFactory {
    private let bootstrapScript: String
    private let hostTelemetry: NativeHostTelemetry?

    init(
        bootstrapScript: String,
        hostTelemetry: NativeHostTelemetry? = nil
    ) {
        self.bootstrapScript = bootstrapScript
        self.hostTelemetry = hostTelemetry
    }

    func makeScripts() throws -> [JSEngineScript] {
        var scripts: [JSEngineScript] = []

        if let hostTelemetry, let telemetryScript = Self.makeHostTelemetryScript(hostTelemetry) {
            scripts.append(telemetryScript)
        }

        try scripts.append(contentsOf: [
            JSEngineScript(content: bootstrapScript, insertionPoint: .atDocStart),
            JSEngineScript(
                content: ContainerScriptBundle.load(),
                insertionPoint: .atDocStart,
                frameScope: .allFrames
            )
        ])

        return scripts
    }

    /// Nil when the telemetry cannot be encoded, so it never stops the product from starting.
    private static func makeHostTelemetryScript(_ telemetry: NativeHostTelemetry) -> JSEngineScript? {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard
            let data = try? encoder.encode(telemetry),
            let json = String(data: data, encoding: .utf8)
        else {
            return nil
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
