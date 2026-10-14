import Foundation

/// The outcome the interactive signing wireframe delivers to the signing
/// context once the sheet has finished dismissing.
enum PolkadotSigningDecision {
    case signed(PolkadotHostSigningResult)
    case rejected
}
