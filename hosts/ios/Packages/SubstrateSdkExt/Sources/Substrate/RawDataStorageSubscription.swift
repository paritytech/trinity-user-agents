import Foundation

public final class RawDataStorageSubscription {
    public let remoteStorageKey: Data
    public let callbackClosure: (Data?, Data?) -> Void

    public init(remoteStorageKey: Data, callbackClosure: @escaping (Data?, Data?) -> Void) {
        self.remoteStorageKey = remoteStorageKey
        self.callbackClosure = callbackClosure
    }
}

extension RawDataStorageSubscription: StorageChildSubscribing {
    public func processUpdate(_ data: Data?, blockHash: Data?) {
        callbackClosure(data, blockHash)
    }
}
