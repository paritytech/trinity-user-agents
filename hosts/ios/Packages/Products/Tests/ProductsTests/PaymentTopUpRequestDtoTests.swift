import Testing
import Foundation
import SubstrateSdk
import BigInt
@testable import Products

struct PaymentTopUpRequestDtoTests {
    private func decode<T: Decodable>(_ jsonString: String, to type: T.Type) throws -> T {
        let json = try JSONDecoder().decode(JSON.self, from: Data(jsonString.utf8))
        return try json.map(to: type)
    }

    /// The container sends the params FLAT: `id`/`amount` sit next to `sourceTag`/`sourceKey…` — there is
    /// no nested `source` object. Guards the wire contract that recently regressed.
    @Test func decodesFlatWireRequestWithCoinsSource() throws {
        let idBytes = Data(repeating: 0xAB, count: 32)
        let dto = try decode(
            """
            {
              "id": "\(idBytes.toHex(includePrefix: true))",
              "amount": "1000",
              "sourceTag": "Coins",
              "sourceKeyListHex": ["0x01", "0x02"]
            }
            """,
            to: PaymentTopUpRequestDto.self
        )
        #expect(dto.id == idBytes)
        #expect(dto.amount == 1_000)
        #expect(dto.source == .coins(secretKeys: [Data([0x01]), Data([0x02])]))
    }

    @Test func decodesFlatWireRequestWithPrivateKeySource() throws {
        let idBytes = Data(repeating: 0x01, count: 32)
        let dto = try decode(
            """
            {
              "id": "\(idBytes.toHex(includePrefix: true))",
              "amount": "5",
              "sourceTag": "PrivateKey",
              "sourceKeyHex": "0x0a0b"
            }
            """,
            to: PaymentTopUpRequestDto.self
        )
        #expect(dto.id == idBytes)
        #expect(dto.source == .privateKey(Data([0x0A, 0x0B])))
    }

    @Test func statusSubscribeDecodesIdFromHex() throws {
        let idBytes = Data(repeating: 0x07, count: 32)
        let dto = try decode(
            "{ \"id\": \"\(idBytes.toHex(includePrefix: true))\" }",
            to: PaymentTopUpStatusSubscribeDto.self
        )
        #expect(dto.id == idBytes)
    }
}
