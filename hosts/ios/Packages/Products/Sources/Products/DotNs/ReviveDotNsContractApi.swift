import Foundation
import os
import Revive
import SubstrateSdk

/// ``DotNsContractApiProtocol`` over pallet-revive: the resolver and name-registry contracts are read
/// through a ``ReviveContractApiProtocol`` bound to the contracts chain.
public final class ReviveDotNsContractApi: Sendable {
    private let contractApi: any ReviveContractApiProtocol
    private let configProvider: @Sendable () throws -> DotNsConfig

    // Every text or content read costs a name-registry lookup first, and one product resolution
    // reads four names. `.some(nil)` is a name the registry has no entry for, cached as
    // deliberately as a hit — legacy names have none and are read constantly.
    private let resolverCache = OSAllocatedUnfairLock(initialState: [String: EvmAddress?]())

    public init(
        contractApi: any ReviveContractApiProtocol,
        configProvider: @Sendable @escaping () throws -> DotNsConfig
    ) {
        self.contractApi = contractApi
        self.configProvider = configProvider
    }
}

private extension ReviveDotNsContractApi {
    /// A record read. A revert with no reason is a resolver that does not implement the function
    /// (Solidity's bare `revert()` on an unknown selector) and reads as no record; a revert with a
    /// reason is the contract refusing and stays an error.
    func callReviveContract(contract: EvmAddress, inputData: Data) async throws -> Data {
        do {
            return try await contractApi.callReadOnly(contract: contract, input: inputData, at: nil)
        } catch let revert as ReviveContractRevertedError where revert.data.isEmpty {
            return Data()
        } catch {
            throw DotNsContractError.contractCallFailed(error)
        }
    }

    /// Where a name's content hash is read from. Legacy names have no registry entry of their own
    /// and keep resolving through the fixed address.
    func contentResolver(for node: Data, dotNsName: String) async throws -> EvmAddress {
        let config = try configProvider()

        return try await nameRegistryResolver(for: node, dotNsName: dotNsName)
            ?? config.resolverContractAddress
    }

    /// Content hash bytes for a name, retried through the fixed resolver when the resolver the
    /// registry named has no code at its address.
    func readContentHash(node: Data, dotNsName: String) async throws -> Data {
        let config = try configProvider()
        let resolver = try await contentResolver(for: node, dotNsName: dotNsName)

        let output = try await callReviveContract(
            contract: resolver,
            inputData: DotNsAbi.encodeContentHash(node: node)
        )

        guard output.isEmpty else { return output }

        guard resolver != config.resolverContractAddress else {
            throw DotNsContractError.contentHashNotFound
        }

        resolverCache.withLock { $0[dotNsName] = nil }

        let fallback = try await callReviveContract(
            contract: config.resolverContractAddress,
            inputData: DotNsAbi.encodeContentHash(node: node)
        )

        guard !fallback.isEmpty else {
            throw DotNsContractError.contentHashNotFound
        }

        return fallback
    }

    /// The resolver a name registered for itself, or nil when it has no registry entry. Callers
    /// that need content specifically want ``contentResolver(for:dotNsName:)`` and its fallback.
    func nameRegistryResolver(for node: Data, dotNsName: String) async throws -> EvmAddress? {
        let config = try configProvider()

        // An unconfigured name registry disables manifest resolution without breaking legacy names.
        guard let registry = config.nameRegistryContractAddress else {
            return nil
        }

        if let cached = resolverCache.withLock({ $0[dotNsName] }) {
            return cached
        }

        let output = try await callReviveContract(
            contract: registry,
            inputData: DotNsAbi.encodeResolver(node: node)
        )

        let resolver = output.isEmpty ? nil : DotNsAbi.decodeResolver(output: output)
        resolverCache.withLock { $0[dotNsName] = .some(resolver) }

        return resolver
    }
}

extension ReviveDotNsContractApi: DotNsContractApiProtocol {
    public func resolveContentHash(dotNsName: String) async throws -> Data {
        let node = try NameHash.nameHash(dotNsName)
        let outputBytes = try await readContentHash(node: node, dotNsName: dotNsName)

        guard let contentHash = DotNsAbi.decodeContentHash(output: outputBytes) else {
            throw DotNsContractError.contentHashNotFound
        }

        return try Eip1577.stripPrefix(contentHash)
    }

    public func getMetadata(dotNsName: String, key: String) async throws -> String? {
        let node = try NameHash.nameHash(dotNsName)

        // Text records live only on a name's own resolver, so a name without one has none.
        guard let resolver = try await nameRegistryResolver(for: node, dotNsName: dotNsName) else {
            return nil
        }

        let outputBytes = try await callReviveContract(
            contract: resolver,
            inputData: DotNsAbi.encodeText(node: node, key: key)
        )

        guard !outputBytes.isEmpty else {
            resolverCache.withLock { $0[dotNsName] = nil }
            return nil
        }

        return DotNsAbi.decodeText(output: outputBytes)
    }

    public func clearCache() {
        resolverCache.withLock { $0.removeAll() }
    }
}
