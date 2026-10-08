import Foundation

/// A country the user can pay from, as ISO 3166-1 alpha-2, with the currency
/// a card or bank payment there is made in.
struct FundingCountry: Identifiable, Hashable {
    let code: String
    let name: String
    let currencyCode: String?
    let currencyName: String?

    var id: String { code }

    /// The regional-indicator pair for the code, which every platform draws as
    /// the country's flag.
    var flag: String {
        code.unicodeScalars
            .compactMap { UnicodeScalar(127_397 + $0.value) }
            .map { String($0) }
            .joined()
    }
}

/// Every ISO 3166-1 country Foundation knows, named in the user's language.
enum FundingCountries {
    static func all(locale: Locale = .current) -> [FundingCountry] {
        Locale.Region.isoRegions
            .map(\.identifier)
            .filter(isAlpha2)
            .compactMap { country(code: $0, locale: locale) }
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    /// The region the device is set to, which the payment country starts at.
    static func detected(locale: Locale = .current) -> FundingCountry? {
        guard let code = locale.region?.identifier, isAlpha2(code) else { return nil }

        return country(code: code, locale: locale)
    }

    static func country(code: String, locale: Locale = .current) -> FundingCountry? {
        let upper = code.uppercased()
        guard let name = locale.localizedString(forRegionCode: upper) else { return nil }

        let currency = Locale(identifier: "en_\(upper)").currency?.identifier
        return FundingCountry(
            code: upper,
            name: name,
            currencyCode: currency,
            currencyName: currency.flatMap { locale.localizedString(forCurrencyCode: $0) }
        )
    }
}

private extension FundingCountries {
    static func isAlpha2(_ code: String) -> Bool {
        code.count == 2 && code.allSatisfy { $0.isASCII && $0.isLetter }
    }
}
