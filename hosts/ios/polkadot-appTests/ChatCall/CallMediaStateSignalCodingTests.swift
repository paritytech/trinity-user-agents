@testable import polkadot_app
import Foundation
import SubstrateSdk
import Testing

/// Byte-level pins for the call media-state signal.
/// Android's `MediaStateSignal` defines this encoding — it is the cross-platform contract.
@Suite("CallMediaStateSignal Coding Tests")
struct CallMediaStateSignalCodingTests {
    @Test(
        "Signals encode as the u8 case index followed by a one-byte Bool",
        arguments: [
            (CallMediaStateSignal.cameraEnabled(true), Data([0x00, 0x01])),
            (CallMediaStateSignal.cameraEnabled(false), Data([0x00, 0x00])),
            (CallMediaStateSignal.microphoneEnabled(true), Data([0x01, 0x01])),
            (CallMediaStateSignal.microphoneEnabled(false), Data([0x01, 0x00]))
        ]
    )
    func scaleLayout(signal: CallMediaStateSignal, bytes: Data) throws {
        #expect(try signal.scaleEncoded() == bytes)
        #expect(try CallMediaStateSignal(scaleDecoder: ScaleDecoder(data: bytes)) == signal)
    }

    @Test("Unknown case index fails to decode")
    func unknownIndexThrows() {
        #expect(throws: (any Error).self) {
            try CallMediaStateSignal(scaleDecoder: ScaleDecoder(data: Data([0x99, 0x01])))
        }
    }
}
