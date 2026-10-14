import Foundation

public final class RootBandersnatchDeriver: BandersnatchEntropyDeriving {
    public init() {}

    public func deriveEntropy(from seed: Data) throws -> Data {
        try seed.blake2b32()
    }
}
