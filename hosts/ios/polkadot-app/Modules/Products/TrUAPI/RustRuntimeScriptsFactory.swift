import Products
import TrUAPIHost

/// Bootstrap must publish the bridge endpoint before the container runs.
final class RustRuntimeScriptsFactory {
    private let bootstrapScript: String

    init(bootstrapScript: String) {
        self.bootstrapScript = bootstrapScript
    }

    func makeScripts() throws -> [JSEngineScript] {
        try [
            JSEngineScript(content: bootstrapScript, insertionPoint: .atDocStart),
            JSEngineScript(
                content: ContainerScriptBundle.load(),
                insertionPoint: .atDocStart,
                frameScope: .allFrames
            )
        ]
    }
}
