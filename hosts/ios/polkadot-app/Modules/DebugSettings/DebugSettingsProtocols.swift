import Foundation
import UIKitExt

protocol DebugSettingsViewProtocol: ControllerBackedProtocol {
    func didReceive(canClearBackup: Bool)
    func didReceive(canClearReferral: Bool)
    func didReceive(hasJWTToken: Bool)
    func didReceive(strategyDebugEnabled: Bool)
    func didReceive(truApiRuntimeEnabled: Bool)
    func didReceive(hostPlacementEnabled: Bool)
}

@MainActor
protocol DebugSettingsPresenterProtocol: AnyObject {
    func setup()
    func clearBackup()
    func clearReferral()
    func clearJWTToken()
    func shareLogs()
    func showProducts()
    func showDotNsBrowser()
    func replaceWithRandomEntropy()
    func showThemeSelection()
    func toggleStrategyDebug()
    func toggleTruApiRuntime()
    func toggleHostPlacement()
    func openTrUAPIPlayground()
    func showPocketFacePreview()
    func showPocketCards()
    func resetTips()
}

protocol DebugSettingsInteractorInputProtocol: AnyObject {
    func setup()
    func clearBackup()
    func clearReferral()
    func clearJWTToken()
    func makeLogsDraft() -> EmailDraft?
    func replaceWithRandomEntropy()
    func toggleStrategyDebug()
    func toggleTruApiRuntime()
    func toggleHostPlacement()
    func restartApp()
    func resetTips()
}

@MainActor
protocol DebugSettingsInteractorOutputProtocol: AnyObject {
    func didReceive(canClearBackup: Bool)
    func didReceive(canClearReferral: Bool)
    func didReceive(hasJWTToken: Bool)
    func didReceive(strategyDebugEnabled: Bool)
    func didReceive(truApiRuntimeEnabled: Bool)
    func didReceive(hostPlacementEnabled: Bool)
}

@MainActor
protocol DebugSettingsWireframeProtocol: AnyObject, AlertPresentable {
    func showProducts(from view: ControllerBackedProtocol?)
    func showDotNsBrowser(from view: ControllerBackedProtocol?)
    func showThemeSelection(from view: ControllerBackedProtocol?)
    func showTrUAPIPlayground(from view: ControllerBackedProtocol?)
    func showPocketFacePreview(from view: ControllerBackedProtocol?)
    func showPocketCards(from view: ControllerBackedProtocol?)
}
