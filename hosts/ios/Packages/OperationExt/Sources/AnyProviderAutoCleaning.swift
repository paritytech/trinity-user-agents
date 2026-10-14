import Foundation
import Operation_iOS

public protocol AnyProviderAutoCleaning {
    func clear(singleValueProvider: inout AnySingleValueProvider<some Any>?)
    func clear(dataProvider: inout AnyDataProvider<some Any>?)
    func clear(streamableProvider: inout StreamableProvider<some Any>?)
}

public extension AnyProviderAutoCleaning where Self: AnyObject {
    func clear(singleValueProvider: inout AnySingleValueProvider<some Any>?) {
        singleValueProvider?.removeObserver(self)
        singleValueProvider = nil
    }

    func clear(dataProvider: inout AnyDataProvider<some Any>?) {
        dataProvider?.removeObserver(self)
        dataProvider = nil
    }

    func clear(streamableProvider: inout StreamableProvider<some Any>?) {
        streamableProvider?.removeObserver(self)
        streamableProvider = nil
    }
}
