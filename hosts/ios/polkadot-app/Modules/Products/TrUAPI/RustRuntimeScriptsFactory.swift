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

        if let hostTelemetry {
            try scripts.append(makeHostTelemetryScript(hostTelemetry))
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
