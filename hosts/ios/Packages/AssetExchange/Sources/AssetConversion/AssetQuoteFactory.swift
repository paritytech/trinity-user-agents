import Foundation
import Operation_iOS

public protocol AssetQuoteFactoryProtocol {
    func quote(for args: AssetConversion.QuoteArgs) -> CompoundOperationWrapper<AssetConversion.Quote>
}
