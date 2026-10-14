import Foundation

public protocol RuntimeProviderPoolProtocol {
    func setupRuntimeProviderIfNeeded(for chain: ChainModel) -> RuntimeProviderProtocol
    func destroyRuntimeProviderIfExists(for chainId: ChainModel.Id)
    func getRuntimeProvider(for chainId: ChainModel.Id) -> RuntimeProviderProtocol?
}

public final class RuntimeProviderPool {
    public let runtimeProviderFactory: RuntimeProviderFactoryProtocol
    private(set) var runtimeProviders: [ChainModel.Id: RuntimeProviderProtocol] = [:]

    private var mutex = NSLock()

    public init(runtimeProviderFactory: RuntimeProviderFactoryProtocol) {
        self.runtimeProviderFactory = runtimeProviderFactory
    }
}

extension RuntimeProviderPool: RuntimeProviderPoolProtocol {
    public func setupRuntimeProviderIfNeeded(for chain: ChainModel) -> RuntimeProviderProtocol {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let runtimeProvider = runtimeProviders[chain.chainId] {
            runtimeProvider.replaceChainData(chain)

            return runtimeProvider
        } else {
            let runtimeProvider = runtimeProviderFactory.createRuntimeProvider(for: chain)

            runtimeProviders[chain.chainId] = runtimeProvider

            runtimeProvider.setup()

            return runtimeProvider
        }
    }

    public func destroyRuntimeProviderIfExists(for chainId: ChainModel.Id) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let runtimeProvider = runtimeProviders[chainId]
        runtimeProvider?.cleanup()

        runtimeProviders[chainId] = nil
    }

    public func getRuntimeProvider(for chainId: ChainModel.Id) -> RuntimeProviderProtocol? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return runtimeProviders[chainId]
    }
}
