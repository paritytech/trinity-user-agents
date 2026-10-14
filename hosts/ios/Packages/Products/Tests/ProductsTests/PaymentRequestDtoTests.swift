import Foundation
import SubstrateSdk
import Testing
@testable import Products

@Suite("PaymentRequestDto Tests")
struct PaymentRequestDtoTests {
    private let idHex = "0x" + String(repeating: "ab", count: 32)
    private let destinationHex = "0x" + String(repeating: "cd", count: 32)

    private func decode<T: Decodable>(_ type: T.Type, json: String) throws -> T {
        try JSONDecoder().decode(type, from: Data(json.utf8))
    }

    @Test("decodes the wire shape: idHex, amount and destinationHex")
    func decodesWireShape() throws {
        let json = #"{"idHex":"\#(idHex)","amount":"1500","destinationHex":"\#(destinationHex)"}"#

        let dto = try decode(PaymentRequestDto.self, json: json)

        #expect(dto.id == Data(repeating: 0xAB, count: 32))
        #expect(dto.amount == 1_500)
        #expect(dto.destination == Data(repeating: 0xCD, count: 32))
    }

    @Test("the id is opaque: any length decodes, a missing key does not")
    func idIsOpaque() throws {
        let dto = try decode(
            PaymentRequestDto.self,
            json: #"{"idHex":"0xab","amount":"1","destinationHex":"\#(destinationHex)"}"#
        )
        #expect(dto.id == Data([0xAB]))

        #expect(throws: DecodingError.self) {
            try decode(PaymentRequestDto.self, json: #"{"amount":"1","destinationHex":"\#(destinationHex)"}"#)
        }
    }

    @Test("status subscribe decodes idHex")
    func statusSubscribeDecodes() throws {
        let dto = try decode(PaymentStatusSubscribeDto.self, json: #"{"idHex":"\#(idHex)"}"#)

        #expect(dto.id == Data(repeating: 0xAB, count: 32))
    }

    @Test("status dto keeps the tagged shape")
    func statusDtoShape() throws {
        let failed = try HostPaymentStatusDto(status: .failed(reason: "boom")).toScaleCompatibleJSON()
        #expect(failed.tag?.stringValue == "Failed")
        #expect(failed.value?.stringValue == "boom")

        let processing = try HostPaymentStatusDto(status: .processing).toScaleCompatibleJSON()
        #expect(processing.tag?.stringValue == "Processing")
        #expect(processing.value == nil)

        let completed = try HostPaymentStatusDto(status: .completed).toScaleCompatibleJSON()
        #expect(completed.tag?.stringValue == "Completed")
        #expect(completed.value == nil)

        let partial = try HostPaymentStatusDto(status: .partiallyClaimed(settledInPlanks: 1_500))
            .toScaleCompatibleJSON()
        #expect(partial.tag?.stringValue == "PartiallyClaimed")
        #expect(partial.value?.stringValue == "1500")
    }

    @Test("status errors are coded")
    func statusErrorCodes() {
        #expect(HostPaymentStatusError.notFound.code == "NotFound")
        #expect(HostPaymentRequestError.alreadyExists.code == "AlreadyExists")
    }
}
