import Foundation
import SubstrateSdk

public protocol ConnectionStateReporting {
    var state: WebSocketEngine.State { get }
}
