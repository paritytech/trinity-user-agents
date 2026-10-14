import Foundation
import SubstrateSdk

public enum MessageExchange {
    public typealias AccountId = Data
    public typealias CodableMessage = Equatable & ScaleCodable
}
