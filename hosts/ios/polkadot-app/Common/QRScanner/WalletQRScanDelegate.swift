import Foundation

@MainActor
protocol WalletQRScanDelegate: AnyObject {
    func walletQRScanDidReceiveURL(_ url: URL)
    func walletQRScanDidReceiveDsfinvkReceipt(_ receipt: W3sDsfinvkReceipt)
}
