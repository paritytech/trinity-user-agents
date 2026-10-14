import KeyDerivation

public protocol BandersnatchKeyResolving {
    var liteKeyManager: any BandersnatchKeyManaging { get }
    var fullKeyManager: any BandersnatchKeyManaging { get }
}

public final class BandersnatchKeyResolver: BandersnatchKeyResolving {
    public let liteKeyManager: any BandersnatchKeyManaging
    public let fullKeyManager: any BandersnatchKeyManaging

    public init(
        liteKeyManager: any BandersnatchKeyManaging,
        fullKeyManager: any BandersnatchKeyManaging
    ) {
        self.liteKeyManager = liteKeyManager
        self.fullKeyManager = fullKeyManager
    }
}
