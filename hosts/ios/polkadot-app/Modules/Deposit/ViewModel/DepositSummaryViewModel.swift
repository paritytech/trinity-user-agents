import UIKit
import Foundation
import PolkadotUI

struct DepositSummaryViewModel {
    let asset: String
    let assetIcon: ImageViewModelProtocol?
    let network: String
    let minimumAmount: String?
    let address: String
    let rateAmountIn: String
    let rateAmountOut: String
    let fee: String
    let qrCodeImage: UIImage?
}
