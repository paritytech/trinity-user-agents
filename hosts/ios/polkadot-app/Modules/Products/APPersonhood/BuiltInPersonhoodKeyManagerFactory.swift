import Foundation
import KeyDerivation

protocol BuiltInPersonhoodKeyManagerMaking: Sendable {
    func makeKeyManager(for slot: BuiltInPersonhoodSlot, tld: String) throws -> BandersnatchKeyManaging
}

final class BuiltInPersonhoodKeyManagerFactory: @unchecked Sendable {
    private let entropyManager: RootEntropyManaging

    init(entropyManager: RootEntropyManaging) {
        self.entropyManager = entropyManager
    }
}

extension BuiltInPersonhoodKeyManagerFactory: BuiltInPersonhoodKeyManagerMaking {
    func makeKeyManager(for slot: BuiltInPersonhoodSlot, tld: String) throws -> BandersnatchKeyManaging {
        switch slot {
        case .fullPerson:
            try BandersnatchKeyManager.fullPerson(for: tld, entropyManager: entropyManager)
        case .litePerson:
            try BandersnatchKeyManager.litePerson(for: tld, entropyManager: entropyManager)
        }
    }
}
