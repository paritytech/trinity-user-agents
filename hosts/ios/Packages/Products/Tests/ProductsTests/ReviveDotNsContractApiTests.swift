import Foundation
import Revive
import SubstrateSdk
import Testing
@testable import Products

struct ReviveDotNsContractApiTests {
    private static let fixedResolver = EvmAddress(repeating: 0xF1, count: EvmAddressFormat.size)
    private static let registry = EvmAddress(repeating: 0x9E, count: EvmAddressFormat.size)
    private static let ownResolver = EvmAddress(repeating: 0x0A, count: EvmAddressFormat.size)
    private static let name = "alice.dot"
    private static let contentHash = Eip1577.ipfsPrefix + Data(repeating: 0xCD, count: 34)

    private static func config(registry: EvmAddress? = ReviveDotNsContractApiTests.registry) -> DotNsConfig {
        DotNsConfig(
            contractsChainId: "asset-hub",
            resolverContractAddress: fixedResolver,
            nameRegistryContractAddress: registry,
            ipfsGatewayBaseUrl: URL(string: "https://ipfs.example")!
        )
    }

    private static func contentHashAnswer(_ hash: Data = contentHash) -> @Sendable (Data) throws -> Data {
        { input in
            input.prefix(4) == Data([0xBC, 0x1C, 0x58, 0xD1]) ? AbiOutput.dynamicBytes(hash) : Data()
        }
    }

    private static func textAnswer(_ value: String) -> @Sendable (Data) throws -> Data {
        { input in
            input.prefix(4) == Data([0x59, 0xD1, 0xD4, 0x3C]) ? AbiOutput.string(value) : Data()
        }
    }

    @Test("a name with its own resolver reads content from it, and the registry only once")
    func ownResolverIsCached() async throws {
        let contracts = StubReviveContractApi(answers: [
            Self.registry: { _ in AbiOutput.address(Self.ownResolver) },
            Self.ownResolver: Self.contentHashAnswer()
        ])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config() }

        let first = try await api.resolveContentHash(dotNsName: Self.name)
        let second = try await api.resolveContentHash(dotNsName: Self.name)

        #expect(first == Data(repeating: 0xCD, count: 34))
        #expect(second == first)
        #expect(contracts.calls(to: Self.registry).count == 1)
        #expect(contracts.calls(to: Self.ownResolver).count == 2)
        #expect(contracts.calls(to: Self.fixedResolver).isEmpty)
    }

    @Test("a name the registry does not know is read from the fixed resolver, and that answer is cached too")
    func legacyNameUsesFixedResolver() async throws {
        let contracts = StubReviveContractApi(answers: [
            Self.registry: { _ in AbiOutput.address(EvmAddressFormat.zero) },
            Self.fixedResolver: Self.contentHashAnswer()
        ])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config() }

        _ = try await api.resolveContentHash(dotNsName: Self.name)
        _ = try await api.resolveContentHash(dotNsName: Self.name)

        #expect(contracts.calls(to: Self.registry).count == 1)
        #expect(contracts.calls(to: Self.fixedResolver).count == 2)
    }

    @Test("a resolver with no code falls back to the fixed resolver and forgets the cached entry")
    func emptyResolverFallsBack() async throws {
        let contracts = StubReviveContractApi(answers: [
            Self.registry: { _ in AbiOutput.address(Self.ownResolver) },
            Self.ownResolver: { _ in Data() },
            Self.fixedResolver: Self.contentHashAnswer()
        ])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config() }

        let content = try await api.resolveContentHash(dotNsName: Self.name)
        _ = try await api.resolveContentHash(dotNsName: Self.name)

        #expect(content == Data(repeating: 0xCD, count: 34))
        #expect(contracts.calls(to: Self.fixedResolver).count == 2)
        #expect(contracts.calls(to: Self.registry).count == 2)
    }

    /// A registry entry can name a contract that is not a resolver at all; Solidity answers an
    /// unknown selector with a bare `revert()`, which is a revert carrying no data.
    @Test("a resolver that reverts without a reason is treated as having no record and falls back")
    func emptyRevertFallsBack() async throws {
        let contracts = StubReviveContractApi(answers: [
            Self.registry: { _ in AbiOutput.address(Self.ownResolver) },
            Self.ownResolver: { _ in throw ReviveContractRevertedError(data: Data()) },
            Self.fixedResolver: Self.contentHashAnswer()
        ])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config() }

        let content = try await api.resolveContentHash(dotNsName: Self.name)
        _ = try await api.resolveContentHash(dotNsName: Self.name)

        #expect(content == Data(repeating: 0xCD, count: 34))
        #expect(contracts.calls(to: Self.fixedResolver).count == 2)
        #expect(contracts.calls(to: Self.registry).count == 2)
    }

    @Test("a resolver that reverts without a reason has no text record")
    func emptyRevertMeansNoMetadata() async throws {
        let contracts = StubReviveContractApi(answers: [
            Self.registry: { _ in AbiOutput.address(Self.ownResolver) },
            Self.ownResolver: { _ in throw ReviveContractRevertedError(data: Data()) }
        ])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config() }

        let url = try await api.getMetadata(dotNsName: Self.name, key: "url")

        #expect(url == nil)
        #expect(contracts.calls(to: Self.fixedResolver).isEmpty)
    }

    @Test("without a registry configured every name resolves through the fixed resolver")
    func noRegistry() async throws {
        let contracts = StubReviveContractApi(answers: [Self.fixedResolver: Self.contentHashAnswer()])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config(registry: nil) }

        _ = try await api.resolveContentHash(dotNsName: Self.name)

        #expect(contracts.calls.map(\.contract) == [Self.fixedResolver])
    }

    @Test("a fixed resolver with nothing for the name is a missing content hash")
    func contentHashNotFound() async throws {
        let contracts = StubReviveContractApi(answers: [Self.fixedResolver: { _ in Data() }])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config(registry: nil) }

        await #expect(throws: DotNsContractError.self) {
            try await api.resolveContentHash(dotNsName: Self.name)
        }
    }

    @Test("a reverted contract read is a failed call, not content")
    func revertIsAnError() async throws {
        let contracts = StubReviveContractApi(answers: [
            Self.fixedResolver: { _ in throw ReviveContractRevertedError(data: Data([0x08, 0xC3, 0x79, 0xA0])) }
        ])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config(registry: nil) }

        do {
            _ = try await api.resolveContentHash(dotNsName: Self.name)
            Issue.record("a revert resolved to content")
        } catch let DotNsContractError.contractCallFailed(underlying) {
            #expect(underlying is ReviveContractRevertedError)
        }
    }

    @Test("text records come from the name's own resolver, and a name without one has none")
    func metadata() async throws {
        let contracts = StubReviveContractApi(answers: [
            Self.registry: { _ in AbiOutput.address(Self.ownResolver) },
            Self.ownResolver: Self.textAnswer("https://alice.example")
        ])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config() }

        let url = try await api.getMetadata(dotNsName: Self.name, key: "url")

        #expect(url == "https://alice.example")
        #expect(contracts.calls(to: Self.ownResolver).count == 1)

        contracts.answer(Self.registry) { _ in AbiOutput.address(EvmAddressFormat.zero) }
        let none = try await api.getMetadata(dotNsName: "bob.dot", key: "url")

        #expect(none == nil)
        #expect(contracts.calls(to: Self.ownResolver).count == 1)
    }

    @Test("clearing the cache makes the next read consult the registry again")
    func clearCache() async throws {
        let contracts = StubReviveContractApi(answers: [
            Self.registry: { _ in AbiOutput.address(Self.ownResolver) },
            Self.ownResolver: Self.contentHashAnswer()
        ])
        let api = ReviveDotNsContractApi(contractApi: contracts) { Self.config() }

        _ = try await api.resolveContentHash(dotNsName: Self.name)
        api.clearCache()
        _ = try await api.resolveContentHash(dotNsName: Self.name)

        #expect(contracts.calls(to: Self.registry).count == 2)
    }
}
