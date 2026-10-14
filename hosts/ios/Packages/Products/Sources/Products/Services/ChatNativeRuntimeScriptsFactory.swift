import Foundation

/// Ordered scripts evaluated in the chat engine before the product module loads.
public protocol ChatScriptsMaking {
    func makeScripts() throws -> [String]
}

/// Native chat runtime scripts: the bundled host container only.
public final class ChatNativeRuntimeScriptsFactory: ChatScriptsMaking {
    private let containerScriptProvider: ContainerScriptProviding

    public init(containerScriptProvider: ContainerScriptProviding = BundledContainerScriptProvider()) {
        self.containerScriptProvider = containerScriptProvider
    }

    public func makeScripts() throws -> [String] {
        try [containerScriptProvider.loadContainerScript()]
    }
}
