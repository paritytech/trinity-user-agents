import Foundation

extension NSPredicate {
    static func prices(for currencyId: Int) -> NSPredicate {
        NSPredicate(format: "%K == %d", #keyPath(CDPrice.currency), currencyId)
    }

    static func price(for priceId: String, currencyId: Int) -> NSPredicate {
        let identifier = PriceData.createIdentifier(for: priceId, currencyId: currencyId)

        return NSPredicate(format: "%K == %@", #keyPath(CDPrice.identifier), identifier)
    }
}
