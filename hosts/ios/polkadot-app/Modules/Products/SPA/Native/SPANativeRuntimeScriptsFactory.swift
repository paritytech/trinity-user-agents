import Foundation
import Products

final class SPANativeRuntimeScriptsFactory: SPAScriptsMaking {
    private let containerScriptProvider: ContainerScriptProviding

    init(containerScriptProvider: ContainerScriptProviding) {
        self.containerScriptProvider = containerScriptProvider
    }

    func makeScripts() throws -> [JSEngineScript] {
        let containerScript = try JSEngineScript(
            content: containerScriptProvider.loadContainerScript(),
            insertionPoint: .atDocStart
        )

        return [containerScript, .disableZoom]
    }
}
