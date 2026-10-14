@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

enum MockDeviceSyncError: Error {
    case connectFailed
    case connectCancelled
    case sendUpdateFailed
    case sendAckFailed
    case updateStreamFailed
    case entityApplicationFailed
}
