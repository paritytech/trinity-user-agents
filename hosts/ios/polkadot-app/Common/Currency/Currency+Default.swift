import Foundation

extension Currency {
    static let usd = Currency(
        id: 0,
        code: "USD",
        name: "United States Dollar",
        symbol: "$",
        category: .fiat,
        isPopular: true,
        coingeckoId: "usd"
    )

    static let supported: [Currency] = [
        .usd,
        Currency(
            id: 1,
            code: "EUR",
            name: "Euro",
            symbol: "\u{20AC}",
            category: .fiat,
            isPopular: true,
            coingeckoId: "eur"
        ),
        Currency(
            id: 2,
            code: "GBP",
            name: "British Pound",
            symbol: "\u{00A3}",
            category: .fiat,
            isPopular: true,
            coingeckoId: "gbp"
        ),
        Currency(
            id: 3,
            code: "JPY",
            name: "Japanese Yen",
            symbol: "\u{00A5}",
            category: .fiat,
            isPopular: true,
            coingeckoId: "jpy"
        ),
        Currency(
            id: 4,
            code: "AUD",
            name: "Australian Dollar",
            symbol: "A$",
            category: .fiat,
            isPopular: true,
            coingeckoId: "aud"
        ),
        Currency(
            id: 5,
            code: "CAD",
            name: "Canadian Dollar",
            symbol: "C$",
            category: .fiat,
            isPopular: true,
            coingeckoId: "cad"
        ),
        Currency(
            id: 6,
            code: "CHF",
            name: "Swiss Franc",
            symbol: "CHF",
            category: .fiat,
            isPopular: true,
            coingeckoId: "chf"
        ),
        Currency(
            id: 7,
            code: "CNY",
            name: "Chinese Yuan",
            symbol: "\u{00A5}",
            category: .fiat,
            isPopular: true,
            coingeckoId: "cny"
        ),
        Currency(
            id: 8,
            code: "KRW",
            name: "South Korean Won",
            symbol: "\u{20A9}",
            category: .fiat,
            isPopular: true,
            coingeckoId: "krw"
        ),
        Currency(
            id: 9,
            code: "INR",
            name: "Indian Rupee",
            symbol: "\u{20B9}",
            category: .fiat,
            isPopular: true,
            coingeckoId: "inr"
        ),
        Currency(
            id: 10,
            code: "BRL",
            name: "Brazilian Real",
            symbol: "R$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "brl"
        ),
        Currency(
            id: 11,
            code: "RUB",
            name: "Russian Ruble",
            symbol: "\u{20BD}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "rub"
        ),
        Currency(
            id: 12,
            code: "TRY",
            name: "Turkish Lira",
            symbol: "\u{20BA}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "try"
        ),
        Currency(
            id: 13,
            code: "SGD",
            name: "Singapore Dollar",
            symbol: "S$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "sgd"
        ),
        Currency(
            id: 14,
            code: "HKD",
            name: "Hong Kong Dollar",
            symbol: "HK$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "hkd"
        ),
        Currency(
            id: 15,
            code: "NZD",
            name: "New Zealand Dollar",
            symbol: "NZ$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "nzd"
        ),
        Currency(
            id: 16,
            code: "ZAR",
            name: "South African Rand",
            symbol: "R",
            category: .fiat,
            isPopular: false,
            coingeckoId: "zar"
        ),
        Currency(
            id: 17,
            code: "MXN",
            name: "Mexican Peso",
            symbol: "MX$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "mxn"
        ),
        Currency(
            id: 18,
            code: "SEK",
            name: "Swedish Krona",
            symbol: "kr",
            category: .fiat,
            isPopular: false,
            coingeckoId: "sek"
        ),
        Currency(
            id: 19,
            code: "NOK",
            name: "Norwegian Krone",
            symbol: "kr",
            category: .fiat,
            isPopular: false,
            coingeckoId: "nok"
        ),
        Currency(
            id: 20,
            code: "DKK",
            name: "Danish Krone",
            symbol: "kr",
            category: .fiat,
            isPopular: false,
            coingeckoId: "dkk"
        ),
        Currency(
            id: 21,
            code: "PLN",
            name: "Polish Zloty",
            symbol: "z\u{0142}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "pln"
        ),
        Currency(
            id: 22,
            code: "CZK",
            name: "Czech Koruna",
            symbol: "K\u{010D}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "czk"
        ),
        Currency(
            id: 23,
            code: "HUF",
            name: "Hungarian Forint",
            symbol: "Ft",
            category: .fiat,
            isPopular: false,
            coingeckoId: "huf"
        ),
        Currency(
            id: 24,
            code: "ILS",
            name: "Israeli Shekel",
            symbol: "\u{20AA}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "ils"
        ),
        Currency(
            id: 25,
            code: "PHP",
            name: "Philippine Peso",
            symbol: "\u{20B1}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "php"
        ),
        Currency(
            id: 26,
            code: "THB",
            name: "Thai Baht",
            symbol: "\u{0E3F}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "thb"
        ),
        Currency(
            id: 27,
            code: "IDR",
            name: "Indonesian Rupiah",
            symbol: "Rp",
            category: .fiat,
            isPopular: false,
            coingeckoId: "idr"
        ),
        Currency(
            id: 28,
            code: "MYR",
            name: "Malaysian Ringgit",
            symbol: "RM",
            category: .fiat,
            isPopular: false,
            coingeckoId: "myr"
        ),
        Currency(
            id: 29,
            code: "PKR",
            name: "Pakistani Rupee",
            symbol: "\u{20A8}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "pkr"
        ),
        Currency(
            id: 30,
            code: "NGN",
            name: "Nigerian Naira",
            symbol: "\u{20A6}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "ngn"
        ),
        Currency(
            id: 31,
            code: "ARS",
            name: "Argentine Peso",
            symbol: "ARS$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "ars"
        ),
        Currency(
            id: 32,
            code: "CLP",
            name: "Chilean Peso",
            symbol: "CLP$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "clp"
        ),
        Currency(
            id: 33,
            code: "COP",
            name: "Colombian Peso",
            symbol: "COL$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "cop"
        ),
        Currency(
            id: 34,
            code: "PEN",
            name: "Peruvian Sol",
            symbol: "S/.",
            category: .fiat,
            isPopular: false,
            coingeckoId: "pen"
        ),
        Currency(
            id: 35,
            code: "UAH",
            name: "Ukrainian Hryvnia",
            symbol: "\u{20B4}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "uah"
        ),
        Currency(
            id: 36,
            code: "VND",
            name: "Vietnamese Dong",
            symbol: "\u{20AB}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "vnd"
        ),
        Currency(
            id: 37,
            code: "TWD",
            name: "Taiwan Dollar",
            symbol: "NT$",
            category: .fiat,
            isPopular: false,
            coingeckoId: "twd"
        ),
        Currency(
            id: 38,
            code: "AED",
            name: "UAE Dirham",
            symbol: "AED",
            category: .fiat,
            isPopular: false,
            coingeckoId: "aed"
        ),
        Currency(
            id: 39,
            code: "SAR",
            name: "Saudi Riyal",
            symbol: "SAR",
            category: .fiat,
            isPopular: false,
            coingeckoId: "sar"
        ),
        Currency(
            id: 40,
            code: "KWD",
            name: "Kuwaiti Dinar",
            symbol: "KWD",
            category: .fiat,
            isPopular: false,
            coingeckoId: "kwd"
        ),
        Currency(
            id: 41,
            code: "BHD",
            name: "Bahraini Dinar",
            symbol: "BHD",
            category: .fiat,
            isPopular: false,
            coingeckoId: "bhd"
        ),
        Currency(
            id: 42,
            code: "QAR",
            name: "Qatari Riyal",
            symbol: "QAR",
            category: .fiat,
            isPopular: false,
            coingeckoId: "qar"
        ),
        Currency(
            id: 43,
            code: "OMR",
            name: "Omani Rial",
            symbol: "OMR",
            category: .fiat,
            isPopular: false,
            coingeckoId: "omr"
        ),
        Currency(
            id: 44,
            code: "EGP",
            name: "Egyptian Pound",
            symbol: "E\u{00A3}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "egp"
        ),
        Currency(
            id: 45,
            code: "KES",
            name: "Kenyan Shilling",
            symbol: "KSh",
            category: .fiat,
            isPopular: false,
            coingeckoId: "kes"
        ),
        Currency(
            id: 46,
            code: "GHS",
            name: "Ghanaian Cedi",
            symbol: "GH\u{20B5}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "ghs"
        ),
        Currency(
            id: 47,
            code: "BDT",
            name: "Bangladeshi Taka",
            symbol: "\u{09F3}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "bdt"
        ),
        Currency(
            id: 48,
            code: "LKR",
            name: "Sri Lankan Rupee",
            symbol: "Rs",
            category: .fiat,
            isPopular: false,
            coingeckoId: "lkr"
        ),
        Currency(
            id: 49,
            code: "MMK",
            name: "Myanmar Kyat",
            symbol: "K",
            category: .fiat,
            isPopular: false,
            coingeckoId: "mmk"
        ),
        Currency(
            id: 50,
            code: "RON",
            name: "Romanian Leu",
            symbol: "lei",
            category: .fiat,
            isPopular: false,
            coingeckoId: "ron"
        ),
        Currency(
            id: 51,
            code: "BGN",
            name: "Bulgarian Lev",
            symbol: "лв",
            category: .fiat,
            isPopular: false,
            coingeckoId: "bgn"
        ),
        Currency(
            id: 52,
            code: "HRK",
            name: "Croatian Kuna",
            symbol: "kn",
            category: .fiat,
            isPopular: false,
            coingeckoId: "hrk"
        ),
        Currency(
            id: 53,
            code: "GEL",
            name: "Georgian Lari",
            symbol: "\u{20BE}",
            category: .fiat,
            isPopular: false,
            coingeckoId: "gel"
        ),
        Currency(
            id: 54,
            code: "UYU",
            name: "Uruguayan Peso",
            symbol: "$U",
            category: .fiat,
            isPopular: false,
            coingeckoId: "uyu"
        )
    ]
}
