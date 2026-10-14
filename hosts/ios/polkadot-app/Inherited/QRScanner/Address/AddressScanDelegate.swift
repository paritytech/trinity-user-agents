import Foundation
import SubstrateSdk

protocol AddressScanDelegate: AnyObject {
    func addressScanDidReceiveRecepient(address: AccountAddress, context: AnyObject?)
}
