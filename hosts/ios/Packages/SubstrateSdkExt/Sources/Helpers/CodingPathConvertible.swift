import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateStorageSubscription

public protocol CodingPathConvertible {
    associatedtype CodingPath
    var name: String { get }
    var moduleName: String { get }
    func callAsFunction() -> CodingPath
}

public protocol CallPathConvertible: CodingPathConvertible {}

public extension CallPathConvertible {
    func callAsFunction() -> CallCodingPath {
        CallCodingPath(
            moduleName: moduleName,
            callName: name
        )
    }
}

// MARK: StoragePathConvertible

public protocol StoragePathConvertible: CodingPathConvertible {}

public extension StoragePathConvertible {
    func callAsFunction() -> StorageCodingPath {
        StorageCodingPath(moduleName: moduleName, itemName: name)
    }
}

// MARK: ConstantPathConvertible

public protocol ConstantPathConvertible: CodingPathConvertible {}

public extension ConstantPathConvertible {
    func callAsFunction() -> ConstantCodingPath {
        ConstantCodingPath(moduleName: moduleName, constantName: name)
    }
}

// MARK: RuntimeCallConvertible

public protocol RuntimeCallConvertible: CodingPathConvertible, Codable {}

public extension RuntimeCallConvertible {
    func callAsFunction() -> RuntimeCall<Self> {
        RuntimeCall(
            moduleName: moduleName,
            callName: name,
            args: self
        )
    }
}

public protocol ViewFunctionCallConvertible: CodingPathConvertible, Codable {}

public extension ViewFunctionCallConvertible {
    func callAsFunction() -> ViewFunctionCodingPath {
        ViewFunctionCodingPath(moduleName: moduleName, functionName: name)
    }
}

public protocol EventPathConvertible: CodingPathConvertible {}

public extension EventPathConvertible {
    func callAsFunction() -> EventCodingPath {
        EventCodingPath(
            moduleName: moduleName,
            eventName: name
        )
    }
}

public protocol SubscriptionRequestConvertible {
    var request: SubscriptionRequestProtocol { get }
}

public extension SubscriptionRequestConvertible {
    func batchStorageRequest(mapping key: String?) -> BatchStorageSubscriptionRequest {
        BatchStorageSubscriptionRequest(innerRequest: request, mappingKey: key)
    }
}
