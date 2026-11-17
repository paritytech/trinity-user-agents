import Foundation
import KeyDerivation

@testable import polkadot_app

final class MockBuiltInPersonhoodKeyManagerFactory: BuiltInPersonhoodKeyManagerMaking, @unchecked Sendable {
    let fullKeyManager = MockBandersnatchKeyManager()
    let liteKeyManager = MockBandersnatchKeyManager()

    func makeKeyManager(for slot: BuiltInPersonhoodSlot, tld _: String) -> BandersnatchKeyManaging {
        switch slot {
        case .fullPerson: fullKeyManager
        case .litePerson: liteKeyManager
        }
    }
}
