import Foundation
import KeyDerivation
import NovaCrypto
import Testing

@testable import polkadot_app

@Suite("Root decision routing")
struct RootRouterTests {
    @Test("theme selection precedes onboarding")
    func themeBeforeOnboarding() throws {
        let resolver = makeResolver(themeSelected: false, hasWallets: false, hasBackup: false, hasUsername: false)
        #expect(try resolver.resolve() == .selectTheme)
    }

    @Test("no wallets, no backup → onboarding")
    func onboarding() throws {
        let resolver = makeResolver(themeSelected: true, hasWallets: false, hasBackup: false, hasUsername: false)
        #expect(try resolver.resolve() == .onboarding)
    }

    @Test("no wallets, has backup → restore from cloud")
    func restoreFromCloud() throws {
        let resolver = makeResolver(themeSelected: true, hasWallets: false, hasBackup: true, hasUsername: false)
        #expect(try resolver.resolve() == .restoreFromCloud)
    }

    @Test("wallets without username → username check")
    func usernameCheck() throws {
        let resolver = makeResolver(themeSelected: true, hasWallets: true, hasBackup: false, hasUsername: false)
        #expect(try resolver.resolve() == .usernameCheck)
    }

    @Test("established user → dashboard")
    func dashboard() throws {
        let resolver = makeResolver(
            themeSelected: true,
            hasWallets: true,
            hasBackup: false,
            hasUsername: true
        )
        #expect(try resolver.resolve() == .dashboard)
    }

    @Test("throwing keystore aborts resolution (interactor maps it to broken)")
    func keystoreThrowAborts() {
        let resolver = SequentialDecisionResolver<RootDestination>(
            gates: [
                RootGate.Theme(storage: FakeThemeStorage(selected: true)),
                RootGate.Wallet(
                    entropyManager: ThrowingEntropyManager(),
                    backupHelper: FakeBackupHelper(hasBackup: false)
                )
            ],
            fallback: .dashboard
        )
        #expect(throws: (any Error).self) {
            try resolver.resolve()
        }
    }

    private func makeResolver(
        themeSelected: Bool,
        hasWallets: Bool,
        hasBackup: Bool,
        hasUsername: Bool
    ) -> SequentialDecisionResolver<RootDestination> {
        SequentialDecisionResolver(
            gates: [
                RootGate.Theme(storage: FakeThemeStorage(selected: themeSelected)),
                RootGate.Wallet(
                    entropyManager: FakeEntropyManager(hasWallets: hasWallets),
                    backupHelper: FakeBackupHelper(hasBackup: hasBackup)
                ),
                RootGate.Username(usernameStorage: FakeUsernameStorage(hasUsername: hasUsername))
            ],
            fallback: .dashboard
        )
    }
}

private struct FakeThemeStorage: ThemeSelectionStoring {
    let selected: Bool
    var hasSelectedTheme: Bool { selected }
    func setSelected() {}
}

private struct FakeEntropyManager: RootEntropyManaging {
    let hasWallets: Bool
    func fetchRootEntropy() throws -> Data { Data() }
    func createRootEntropy(_: Data) throws {}
    func hasRootEntropy() throws -> Bool { hasWallets }
}

private struct ThrowingEntropyManager: RootEntropyManaging {
    private struct Failure: Error {}
    func fetchRootEntropy() throws -> Data { throw Failure() }
    func createRootEntropy(_: Data) throws { throw Failure() }
    func hasRootEntropy() throws -> Bool { throw Failure() }
}

private struct FakeBackupHelper: MnemonicBackupHelperProtocol {
    let hasBackup: Bool
    var isAvailable: Bool { true }
    var didChangeAvailabilityNotification: Notification.Name { .init("test.backup") }
    func checkForBackup() throws -> Bool { hasBackup }
    func saveMnemonic(_: IRMnemonicProtocol) throws {}
    func fetchMnemonic() throws -> IRMnemonicProtocol { throw FakeError.unused }
    func deleteMnemonic() throws {}

    private enum FakeError: Error { case unused }
}

private final class FakeUsernameStorage: UsernameStoring {
    var username: Username?
    var usernameClaimed = false
    var isPerson = false

    init(hasUsername: Bool) {
        username = hasUsername ? Username(value: "tester.01") : nil
    }
}
