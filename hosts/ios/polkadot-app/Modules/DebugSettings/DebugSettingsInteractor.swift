import Foundation
import Operation_iOS
import Keystore_iOS
import KeyDerivation
import NovaCrypto
import StructuredConcurrency

final class DebugSettingsInteractor {
    weak var presenter: DebugSettingsInteractorOutputProtocol?

    let mnemonicBackupHelper: MnemonicBackupHelperProtocol
    let logsDraftFactory: LogsEmailDraftMaking
    let keystore: KeystoreProtocol
    let entropyManager: RootEntropyManaging

    init(
        mnemonicBackupHelper: MnemonicBackupHelperProtocol,
        logsDraftFactory: LogsEmailDraftMaking,
        keystore: KeystoreProtocol,
        entropyManager: RootEntropyManaging
    ) {
        self.mnemonicBackupHelper = mnemonicBackupHelper
        self.logsDraftFactory = logsDraftFactory
        self.keystore = keystore
        self.entropyManager = entropyManager
    }
}

extension DebugSettingsInteractor: DebugSettingsInteractorInputProtocol {
    func setup() {
        Task { @MainActor in
            provideClearBackup()
            provideClearReferral()
            provideJWTTokenState()
            provideStrategyDebugState()
            provideTruApiRuntimeState()
            provideHostPlacementState()
        }
    }

    func clearBackup() {
        Task { [weak self, mnemonicBackupHelper, keystore] in
            try? await ClosureOperation {
                try? mnemonicBackupHelper.deleteMnemonic()
                JWTTokenStore(keychain: keystore, sessionIdStore: BackendSessionIdStore()).deleteAll()
            }
            .asyncExecute()
            self?.provideClearBackup()
        }
    }

    func clearReferral() {
        Task { [weak self, keystore] in
            try? await ClosureOperation {
                try keystore.deleteKey(for: KeystoreTag.receivedRefferalTag())
            }
            .asyncExecute()
            self?.provideClearReferral()
        }
    }

    func clearJWTToken() {
        JWTTokenStore(keychain: keystore, sessionIdStore: BackendSessionIdStore()).deleteAll()
        provideJWTTokenState()
    }

    func makeLogsDraft() -> EmailDraft? {
        logsDraftFactory.makeLogsDraft()
    }

    func replaceWithRandomEntropy() {
        do {
            let mnemonic = try IRMnemonicCreator().randomMnemonic(.entropy128)
            try entropyManager.createRootEntropy(mnemonic.entropy())
        } catch {
            Logger.shared.error("Failed to generate entropy: \(error)")
        }
    }

    func toggleStrategyDebug() {
        let current = SettingsManager.shared.bool(for: SettingsKey.showTransferStrategyDebug.rawValue) ?? true
        SettingsManager.shared.set(value: !current, for: SettingsKey.showTransferStrategyDebug.rawValue)
        provideStrategyDebugState()
    }

    func toggleTruApiRuntime() {
        let current = SettingsManager.shared.isTrUAPIRuntimeEnabled
        SettingsManager.shared.set(value: !current, for: .truApiRuntimeEnabled)
        provideTruApiRuntimeState()
    }

    func toggleHostPlacement() {
        let current = SettingsManager.shared.isHostPlacementEnabled
        SettingsManager.shared.set(value: !current, for: .hostPlacementEnabled)
        provideHostPlacementState()
    }

    func restartApp() {
        Logger.shared.info("Debug settings changed — terminating for restart")
        exit(0)
    }

    func resetTips() {
        #if TESTNET_FEATURE
            SettingsManager.shared.set(value: true, for: .tipsResetPending)
        #endif
    }
}

private extension DebugSettingsInteractor {
    func provideClearBackup() {
        Task { [weak presenter] in
            guard let hasBackup = try? mnemonicBackupHelper.checkForBackup() else {
                return
            }
            await presenter?.didReceive(canClearBackup: hasBackup)
        }
    }

    func provideClearReferral() {
        Task { [weak presenter] in
            guard let hasBackup = try? keystore.checkKey(for: KeystoreTag.receivedRefferalTag()) else {
                return
            }
            await presenter?.didReceive(canClearReferral: hasBackup)
        }
    }

    func provideJWTTokenState() {
        Task { [weak presenter] in
            let hasToken = JWTTokenStore(keychain: keystore, sessionIdStore: BackendSessionIdStore())
                .fetchToken() != nil
            await presenter?.didReceive(hasJWTToken: hasToken)
        }
    }

    func provideStrategyDebugState() {
        let enabled = SettingsManager.shared.bool(for: SettingsKey.showTransferStrategyDebug.rawValue) ?? true
        Task { @MainActor [weak presenter] in
            presenter?.didReceive(strategyDebugEnabled: enabled)
        }
    }

    func provideTruApiRuntimeState() {
        let enabled = SettingsManager.shared.isTrUAPIRuntimeEnabled
        Task { @MainActor [weak presenter] in
            presenter?.didReceive(truApiRuntimeEnabled: enabled)
        }
    }

    func provideHostPlacementState() {
        let enabled = SettingsManager.shared.isHostPlacementEnabled
        Task { @MainActor [weak presenter] in
            presenter?.didReceive(hostPlacementEnabled: enabled)
        }
    }
}
