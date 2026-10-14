import Foundation

@MainActor
protocol RootStateObserving: AnyObject {
    func didCreateWallets()
    func didRestoreWallets()
    func didDecideBroken()
    func didClaimUsername()
    func didDecideClaim()
    func didSelectTheme()
}
