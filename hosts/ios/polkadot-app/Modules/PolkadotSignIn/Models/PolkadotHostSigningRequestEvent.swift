import Foundation

struct PolkadotHostSigningRequestEvent {
    let signingModel: PolkadotHostSigningModel
    let host: PolkadotSignInHost
    let requestMessageId: String
}
