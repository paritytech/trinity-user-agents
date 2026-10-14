import BigInt
import Foundation
import SubstrateSdk

/// `ReviveApi_call` as a runtime declares it, built through the metadata's own SCALE form since the
/// SDK gives the method types no other initialiser.
enum RuntimeApiMethodFixture {
    static let callName = "ReviveApi_call"

    static func call(
        inputs: [(name: String, type: SiLookupId)],
        output: SiLookupId = 99
    ) throws -> RuntimeApiQueryResult {
        let encoder = ScaleEncoder()
        try "call".encode(scaleEncoder: encoder)
        try BigUInt(inputs.count).encode(scaleEncoder: encoder)
        for input in inputs {
            try input.name.encode(scaleEncoder: encoder)
            try BigUInt(input.type).encode(scaleEncoder: encoder)
        }
        try BigUInt(output).encode(scaleEncoder: encoder)
        try [String]().encode(scaleEncoder: encoder)

        let method = try RuntimeApiMethodMetadata(scaleDecoder: ScaleDecoder(data: encoder.encode()))
        return RuntimeApiQueryResult(callName: callName, method: method)
    }

    /// The signature after pallet-revive's rename.
    static let currentInputs: [(name: String, type: SiLookupId)] = [
        ("origin", 1), ("dest", 2), ("value", 3), ("weight_limit", 4), ("storage_deposit_limit", 5), ("input_data", 6)
    ]
}
