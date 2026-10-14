import Foundation
import SubstrateSdk

public protocol RawPublicKeyProviding {
    func getRawPublicKey() throws -> Data
}

public protocol RawKeypairSigning: RawPublicKeyProviding {
    func sign(_ data: Data) throws -> Data
}

public protocol RawKeypairProviding: RawPublicKeyProviding {
    func fetchRawSecretKey() throws -> Data
}

public protocol TypedSigningProviding {
    func getMultiSigner() throws -> MultiSigner

    func sign(data: Data) throws -> MultiSignature
}
