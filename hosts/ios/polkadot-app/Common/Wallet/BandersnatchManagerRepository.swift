import Foundation
import KeyDerivation
import Individuality
import Products

/// Resolves personhood ring-VRF key managers against the cached DotNs TLD. Throws if the TLD is
/// missing (can't happen after onboarding, where the TLD is resolved and cached).
protocol BandersnatchManagerRepositoryProtocol {
    func fullPerson() throws -> BandersnatchKeyManager
    func litePerson() throws -> BandersnatchKeyManager
}

extension BandersnatchManagerRepositoryProtocol {
    func keyResolver() throws -> BandersnatchKeyResolving {
        try BandersnatchKeyResolver(
            liteKeyManager: litePerson(),
            fullKeyManager: fullPerson()
        )
    }
}

struct BandersnatchManagerRepository: BandersnatchManagerRepositoryProtocol {
    private let tldProvider: DotNsTldProviding
    private let entropyManager: RootEntropyManaging

    init(
        tldProvider: DotNsTldProviding,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared
    ) {
        self.tldProvider = tldProvider
        self.entropyManager = entropyManager
    }

    func fullPerson() throws -> BandersnatchKeyManager {
        try BandersnatchKeyManager.fullPerson(
            for: tldProvider.currentTldOrError(),
            entropyManager: entropyManager
        )
    }

    func litePerson() throws -> BandersnatchKeyManager {
        try BandersnatchKeyManager.litePerson(
            for: tldProvider.currentTldOrError(),
            entropyManager: entropyManager
        )
    }
}

extension BandersnatchManagerRepositoryProtocol where Self == BandersnatchManagerRepository {
    static var shared: BandersnatchManagerRepositoryProtocol {
        BandersnatchManagerRepository(tldProvider: DotNsTldProviderFacade.shared)
    }
}
