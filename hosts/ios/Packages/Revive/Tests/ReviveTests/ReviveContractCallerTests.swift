import BigInt
import Foundation
import Operation_iOS
import SubstrateSdk
import Testing
@testable import Revive

struct ReviveContractCallerTests {
    private static let origin = Data(repeating: 0xAA, count: 32)
    private static let contract = EvmAddress(repeating: 0xC0, count: EvmAddressFormat.size)
    private static let input = Data([0x01, 0x02])
    private static let arguments = ReviveContractCaller.CallArguments(origin: origin, contract: contract, input: input)

    /// Byte vectors reach the dynamic encoder as one string per byte.
    private static func bytes(_ data: Data) -> JSON {
        .arrayValue(data.map { .stringValue(String($0)) })
    }

    private func encode(inputs: [(name: String, type: SiLookupId)]) throws -> [RecordingScaleEncoder.Entry] {
        let encoder = RecordingScaleEncoder()
        try ReviveContractCaller.encode(
            Self.arguments,
            for: RuntimeApiMethodFixture.call(inputs: inputs),
            into: encoder,
            context: RuntimeJsonContext(prefersRawAddress: true)
        )
        return encoder.entries
    }

    @Test("every parameter is encoded under the type the runtime declares for its name")
    func encodesByName() throws {
        let entries = try encode(inputs: RuntimeApiMethodFixture.currentInputs)
        let maxWeight = String(ReviveContractCaller.maxWeight)

        #expect(entries.map(\.type) == ["1", "2", "3", "4", "5", "6"])
        #expect(entries[0].json == Self.bytes(Self.origin))
        #expect(entries[1].json == Self.bytes(Self.contract))
        #expect(entries[2].json == .stringValue("0"))
        #expect(entries[3].json == .dictionaryValue([
            "refTime": .stringValue(maxWeight),
            "proofSize": .stringValue(maxWeight)
        ]))
        #expect(entries[4].json == .stringValue(maxWeight))
        #expect(entries[5].json == Self.bytes(Self.input))
    }

    @Test("a runtime that declares the parameters in another order still gets each under its own type")
    func reorderedSignature() throws {
        let reordered: [(name: String, type: SiLookupId)] = [
            ("input_data", 6), ("storage_deposit_limit", 5), ("weight_limit", 4), ("value", 3), ("dest", 2), (
                "origin",
                1
            )
        ]

        let entries = try encode(inputs: reordered)

        #expect(entries.map(\.type) == ["1", "2", "3", "4", "5", "6"])
    }

    @Test("a runtime from before the rename takes the weight limit as gas_limit")
    func legacyWeightLimitName() throws {
        let legacy: [(name: String, type: SiLookupId)] = [
            ("origin", 1), ("dest", 2), ("value", 3), ("gas_limit", 40), ("storage_deposit_limit", 5), ("input_data", 6)
        ]

        let entries = try encode(inputs: legacy)

        #expect(entries[3].type == "40")
    }

    @Test("a signature missing a parameter is refused rather than encoded by position")
    func missingParameter() throws {
        let missingDest: [(name: String, type: SiLookupId)] = [
            ("origin", 1), ("value", 3), ("weight_limit", 4), ("storage_deposit_limit", 5), ("input_data", 6)
        ]

        #expect(throws: ReviveContractError.self) { try encode(inputs: missingDest) }
    }

    @Test("the mapping check reads Revive.OriginalAccount keyed by the account's H160")
    func mappingRead() async throws {
        let chainId = "asset-hub"
        let storage = StubStorageRequestFactory()
        let caller = ReviveContractCaller(
            chainResource: StubChainResource(
                chainId: chainId,
                connection: StubJSONRPCEngine(),
                runtimeService: StubRuntimeCodingService()
            ),
            operationQueue: OperationQueue(),
            storageRequestFactory: storage
        )

        await #expect(throws: StubError.storageUnavailable) {
            try await caller.isAccountMapped(chainId: chainId, account: Self.origin)
        }

        #expect(try storage.queries == [
            .init(
                path: StorageCodingPath(moduleName: "Revive", itemName: "OriginalAccount"),
                keys: [BytesCodable(wrappedValue: Self.origin.toH160())]
            )
        ])
    }

    @Test("an unknown chain fails before anything is read")
    func unknownChain() async throws {
        let storage = StubStorageRequestFactory()
        let caller = ReviveContractCaller(
            chainResource: StubChainResource(
                chainId: "other",
                connection: StubJSONRPCEngine(),
                runtimeService: StubRuntimeCodingService()
            ),
            operationQueue: OperationQueue(),
            storageRequestFactory: storage
        )

        await #expect(throws: (any Error).self) {
            try await caller.isAccountMapped(chainId: "asset-hub", account: Self.origin)
        }
        #expect(storage.queries.isEmpty)
    }
}
