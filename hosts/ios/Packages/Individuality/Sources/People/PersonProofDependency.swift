import Foundation
import KeyDerivation

public struct PersonProofDependency {
    public let origin: PersonOrigin
    public let keyManager: BandersnatchKeyManaging
    public let proofParamsFetcher: RingProofParamsProviding

    public init(
        origin: PersonOrigin,
        keyManager: BandersnatchKeyManaging,
        proofParamsFetcher: RingProofParamsProviding
    ) {
        self.origin = origin
        self.keyManager = keyManager
        self.proofParamsFetcher = proofParamsFetcher
    }
}
