import Foundation
import SubstrateSdk
import Testing
@testable import polkadot_app

@Suite("Custom rendered message coding")
struct CustomRenderedDataCodingTests {
    /// Cards stored before `alt` existed end after their data, and must still load.
    @Test
    func aCardStoredWithoutAltDecodesWithNone() throws {
        let encoder = ScaleEncoder()
        try "results".encode(scaleEncoder: encoder)
        try UInt8(7).encode(scaleEncoder: encoder)
        try Data([1, 2]).encode(scaleEncoder: encoder)

        let decoded = try Chat.LocalMessage.Content.CustomRenderedData(
            scaleDecoder: ScaleDecoder(data: encoder.encode())
        )

        #expect(decoded == Chat.LocalMessage.Content.CustomRenderedData(
            decoderId: 7,
            data: Data([1, 2]),
            identifier: "results"
        ))
    }

    @Test
    func aCardKeepsItsAltThroughStorage() throws {
        let card = Chat.LocalMessage.Content.CustomRenderedData(
            decoderId: 7,
            data: Data([1, 2]),
            identifier: "results",
            alt: "Week 12 results"
        )
        let encoder = ScaleEncoder()
        try card.encode(scaleEncoder: encoder)

        let decoded = try Chat.LocalMessage.Content.CustomRenderedData(
            scaleDecoder: ScaleDecoder(data: encoder.encode())
        )

        #expect(decoded == card)
    }
}
