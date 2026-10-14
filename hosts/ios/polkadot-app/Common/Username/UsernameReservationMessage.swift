import Foundation
import SubstrateSdk

/// Wire message signed by the candidate to prove a dotNS username reservation request.
/// Byte layout is pinned to the gateway runtime: `(prefix, candidate, attester, usernameBase,
/// chatKey, reservedBaseLabel, signedAt).encode()`. Candidate precedes attester — do not reorder.
struct UsernameReservationMessage {
    static let prefix = "pop:dotns-gateway:reserve"

    let candidate: AccountId
    let attester: AccountId
    let usernameBase: String
    let chatKey: Chat.OnChainEncryptionIdentifier
    let reservedBaseLabel: Data?
    let signedAt: UInt64
}

extension UsernameReservationMessage: ScaleEncodable {
    func encode(scaleEncoder: any ScaleEncoding) throws {
        try Data(Self.prefix.utf8).encode(scaleEncoder: scaleEncoder)

        scaleEncoder.appendRaw(data: candidate)
        scaleEncoder.appendRaw(data: attester)

        try Data(usernameBase.utf8).encode(scaleEncoder: scaleEncoder)

        try chatKey.scaleEncoded().encode(scaleEncoder: scaleEncoder)

        try ScaleOption(value: reservedBaseLabel).encode(scaleEncoder: scaleEncoder)

        try signedAt.encode(scaleEncoder: scaleEncoder)
    }
}
