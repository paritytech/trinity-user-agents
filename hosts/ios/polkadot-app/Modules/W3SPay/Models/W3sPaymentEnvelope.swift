import Foundation
import SubstrateSdk

/// Wire layout mirrors `AppHandshakeData.DataV1`: length-prefixed ciphertext
/// followed by the raw 32-byte X25519 public key.
struct W3sPaymentEnvelope: ScaleEncodable {
    let encryptedData: Data
    let ephemeralPublicKey: Data

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try encryptedData.encode(scaleEncoder: scaleEncoder)
        scaleEncoder.appendRaw(data: ephemeralPublicKey)
    }
}
