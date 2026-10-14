import BigInt
import Foundation
import SubstrateSdk
import Testing

@testable import Revive

struct ReviveDryRunResultTests {
    @Test("a runtime after the rename reports the weight as weight_required")
    func weightRequired() throws {
        let result = try decode(weightKey: "weightRequired")

        #expect(result.weightRequired == Substrate.WeightV2(refTime: 11, proofSize: 22))
        #expect(result.storageDeposit.charged == 5)
    }

    @Test("a runtime before the rename still reports it as gas_required")
    func gasRequired() throws {
        let result = try decode(weightKey: "gasRequired")

        #expect(result.weightRequired == Substrate.WeightV2(refTime: 11, proofSize: 22))
    }

    @Test("a result with neither spelling is rejected rather than read as zero weight")
    func missingWeight() {
        #expect(throws: DecodingError.self) { try decode(weightKey: nil) }
    }
}

private extension ReviveDryRunResultTests {
    /// The shape the dynamic SCALE decoder hands to `Decodable`: field names camel-cased by the runtime
    /// registry's `ScaleInfoCamelCaseMapper`, structs as objects, enums as `[variant, payload]`, integers
    /// as strings, byte vectors as arrays of byte strings.
    func decode(weightKey: String?) throws -> ReviveDryRunResult {
        var json: [String: Any] = [
            "storageDeposit": ["Charge", "5"],
            "result": ["Ok", ["flags": ["bits": "0"], "data": ["1", "2"]]]
        ]
        if let weightKey {
            json[weightKey] = ["refTime": "11", "proofSize": "22"]
        }
        let data = try JSONSerialization.data(withJSONObject: json)
        return try JSONDecoder().decode(ReviveDryRunResult.self, from: data)
    }
}
