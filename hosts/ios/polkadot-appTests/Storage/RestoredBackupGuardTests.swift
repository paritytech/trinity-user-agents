import Foundation
import KeyDerivation
import Keystore_iOS
import Testing

@testable import polkadot_app

struct RestoredBackupGuardTests {
    private let keyIdStore = MockInstallationKeyIdStore()
    private let eraser = RecordingLocalStateEraser()

    @Test("a fresh install has no key id and nothing is erased")
    func freshInstall() throws {
        let guardStep = makeGuard(keychain: InMemoryKeychain())

        try guardStep.migrate()

        #expect(eraser.calls == 0)
    }

    @Test("a key id whose entropy is in the Keychain is left alone")
    func consistentInstall() throws {
        let keychain = InMemoryKeychain()
        let entropyManager = RootEntropyManager(keychain: keychain, installationKeyIdStore: keyIdStore)
        try entropyManager.createRootEntropy(Data(repeating: 0x01, count: 32))

        try makeGuard(keychain: keychain).migrate()

        #expect(eraser.calls == 0)
    }

    @Test("a key id without entropy erases the wallet state and keeps the key id")
    func restoredOntoAnotherDevice() throws {
        keyIdStore.saveInstallationKeyId("restored-from-backup")

        try makeGuard(keychain: InMemoryKeychain()).migrate()

        #expect(eraser.calls == 1)
        #expect(keyIdStore.getInstallationKeyId() == "restored-from-backup")
    }

    @Test("a Keychain read failure propagates and erases nothing")
    func keychainFailure() throws {
        keyIdStore.saveInstallationKeyId("unknown-state")

        #expect(throws: ThrowingKeychain.Error.unavailable) {
            try makeGuard(keychain: ThrowingKeychain()).migrate()
        }
        #expect(eraser.calls == 0)
    }
}

private extension RestoredBackupGuardTests {
    func makeGuard(keychain: KeystoreProtocol) -> RestoredBackupGuard {
        RestoredBackupGuard(
            keyIdStore: keyIdStore,
            entropyManager: RootEntropyManager(keychain: keychain, installationKeyIdStore: keyIdStore),
            eraser: eraser,
            logger: StubLogger()
        )
    }

    final class RecordingLocalStateEraser: LocalStateErasing {
        private(set) var calls = 0

        func eraseUserState() {
            calls += 1
        }
    }

    final class ThrowingKeychain: KeystoreProtocol {
        enum Error: Swift.Error, Equatable {
            case unavailable
        }

        func addKey(_: Data, with _: String) throws { throw Error.unavailable }
        func updateKey(_: Data, with _: String) throws { throw Error.unavailable }
        func fetchKey(for _: String) throws -> Data { throw Error.unavailable }
        func checkKey(for _: String) throws -> Bool { throw Error.unavailable }
        func deleteKey(for _: String) throws { throw Error.unavailable }
    }
}
