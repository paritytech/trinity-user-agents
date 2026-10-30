import ChainRegistry
import Foundation

/// The pool sleeps unretained connections on backgrounding and nothing wakes them during a background launch.
protocol BackgroundConnectionRetaining: Sendable {
    func retainAllConnections() -> any ConnectionTokenProtocol
}

/// Creating the registry opens the chain store, so it is reached only after launch migrations.
struct ChainRegistryConnectionRetention: BackgroundConnectionRetaining {
    func retainAllConnections() -> any ConnectionTokenProtocol {
        ChainRegistryFacade.sharedRegistry.retainConnections(.all)
    }
}
