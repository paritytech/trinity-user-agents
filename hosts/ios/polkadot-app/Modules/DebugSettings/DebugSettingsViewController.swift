import UIKit
import FoundationExt

final class DebugSettingsViewController: UIViewController, ViewHolder {
    typealias RootViewType = DebugSettingsViewLayout

    let presenter: DebugSettingsPresenterProtocol

    init(presenter: DebugSettingsPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = DebugSettingsViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        title = "Debug"

        setupHandlers()

        presenter.setup()
    }

    private func setupHandlers() {
        rootView.clearBackupButton.addTarget(
            self,
            action: #selector(actionClearBackup),
            for: .touchUpInside
        )

        rootView.clearReferralButton.addTarget(
            self,
            action: #selector(actionClearReferral),
            for: .touchUpInside
        )

        rootView.shareLogsButton.addTarget(
            self,
            action: #selector(actionShareLogs),
            for: .touchUpInside
        )

        rootView.productsButton.addTarget(
            self,
            action: #selector(actionShowProducts),
            for: .touchUpInside
        )

        rootView.dotNsBrowserButton.addTarget(
            self,
            action: #selector(actionShowDotNsBrowser),
            for: .touchUpInside
        )

        rootView.clearJWTTokenButton.addTarget(
            self,
            action: #selector(actionClearJWTToken),
            for: .touchUpInside
        )

        rootView.simulateCrash.addTarget(
            self,
            action: #selector(simulateCrash),
            for: .touchUpInside
        )

        rootView.replaceEntropyButton.addTarget(
            self,
            action: #selector(actionReplaceEntropy),
            for: .touchUpInside
        )

        rootView.themeSelectionButton.addTarget(
            self,
            action: #selector(actionShowThemeSelection),
            for: .touchUpInside
        )

        rootView.resetTipsButton.addTarget(
            self,
            action: #selector(actionResetTips),
            for: .touchUpInside
        )

        rootView.strategyDebugSwitch.addTarget(
            self,
            action: #selector(actionToggleStrategyDebug),
            for: .valueChanged
        )

        rootView.truApiRuntimeSwitch.addTarget(
            self,
            action: #selector(actionToggleTruApiRuntime),
            for: .valueChanged
        )

        rootView.hostPlacedSwitch.addTarget(
            self,
            action: #selector(actionToggleHostPlacement),
            for: .valueChanged
        )

        #if DEBUG
            rootView.openTrUAPIPlaygroundButton.addTarget(
                self,
                action: #selector(actionOpenTrUAPIPlayground),
                for: .touchUpInside
            )

            rootView.pocketFacePreviewButton.addTarget(
                self,
                action: #selector(actionShowPocketFacePreview),
                for: .touchUpInside
            )

            rootView.pocketCardsButton.addTarget(
                self,
                action: #selector(actionShowPocketCards),
                for: .touchUpInside
            )
        #endif
    }

    @objc func actionClearBackup() {
        presenter.clearBackup()
    }

    @objc func actionClearReferral() {
        presenter.clearReferral()
    }

    @objc func actionShareLogs() {
        presenter.shareLogs()
    }

    @objc func actionShowProducts() {
        presenter.showProducts()
    }

    @objc func actionShowDotNsBrowser() {
        presenter.showDotNsBrowser()
    }

    @objc func actionClearJWTToken() {
        presenter.clearJWTToken()
    }

    @objc func simulateCrash() {
        let array = [0]
        _ = array[1]
    }

    @objc func actionReplaceEntropy() {
        presenter.replaceWithRandomEntropy()
    }

    @objc func actionShowThemeSelection() {
        presenter.showThemeSelection()
    }

    @objc func actionResetTips() {
        presenter.resetTips()
    }

    @objc func actionToggleStrategyDebug() {
        presenter.toggleStrategyDebug()
    }

    @objc func actionToggleTruApiRuntime() {
        presenter.toggleTruApiRuntime()
    }

    @objc func actionToggleHostPlacement() {
        presenter.toggleHostPlacement()
    }

    @objc func actionOpenTrUAPIPlayground() {
        presenter.openTrUAPIPlayground()
    }

    @objc func actionShowPocketFacePreview() {
        presenter.showPocketFacePreview()
    }

    @objc func actionShowPocketCards() {
        presenter.showPocketCards()
    }
}

extension DebugSettingsViewController: DebugSettingsViewProtocol {
    func didReceive(canClearBackup: Bool) {
        rootView.setupButtonEnabled(rootView.clearBackupButton, isEnabled: canClearBackup)
    }

    func didReceive(canClearReferral: Bool) {
        rootView.setupButtonEnabled(rootView.clearReferralButton, isEnabled: canClearReferral)
    }

    func didReceive(hasJWTToken: Bool) {
        rootView.clearJWTTokenButton.setTitle(
            hasJWTToken ? "Clear JWT Token (stored)" : "Clear JWT Token (none)"
        )
        rootView.setupButtonEnabled(rootView.clearJWTTokenButton, isEnabled: hasJWTToken)
    }

    func didReceive(strategyDebugEnabled: Bool) {
        rootView.strategyDebugSwitch.isOn = strategyDebugEnabled
    }

    func didReceive(hostPlacementEnabled: Bool) {
        rootView.hostPlacedSwitch.isOn = hostPlacementEnabled
    }

    func didReceive(truApiRuntimeEnabled: Bool) {
        rootView.truApiRuntimeSwitch.isOn = truApiRuntimeEnabled
    }
}
