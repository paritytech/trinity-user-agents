import Foundation
import SubstrateSdk
import Operation_iOS

enum MeldFiatOnrampSupport {}

extension MeldFiatOnrampSupport {
    enum QueryBuilder {
        static func limitsItems(for request: FiatPurchaseLimitsRequest) -> [URLQueryItem] {
            var items = serviceProviderCommonItems(
                filters: request.filters
            )
            appendQueryItem(name: "includeDetails", value: request.includeDetails, to: &items)
            return items
        }

        static func serviceProvidersItems(for request: ServiceProvidersRequest) -> [URLQueryItem] {
            serviceProviderCommonItems(
                filters: request.filters
            )
        }

        static func transactionsItems(for request: FiatOnrampTransactionQuery) -> [URLQueryItem] {
            var items: [URLQueryItem] = []

            appendQueryItems(name: "externalSessionIds", values: request.sessionIds.map(\.value), to: &items)

            return items
        }

        private static func serviceProviderCommonItems(
            filters: ServiceProviderFilters
        ) -> [URLQueryItem] {
            var items: [URLQueryItem] = []

            appendQueryItems(name: "serviceProviders", values: filters.serviceProviders, to: &items)
            appendQueryItems(name: "statuses", values: filters.statuses, to: &items)
            appendQueryItems(name: "categories", values: filters.categories, to: &items)
            appendQueryItem(name: "accountFilter", value: filters.accountFilter, to: &items)
            appendQueryItems(name: "countries", values: filters.countries, to: &items)
            appendQueryItems(name: "fiatCurrencies", values: filters.fiatCurrencies, to: &items)
            appendQueryItems(name: "cryptoChains", values: filters.cryptoChains, to: &items)
            appendQueryItems(name: "cryptoCurrencies", values: filters.cryptoCurrencies, to: &items)

            return items
        }

        private static func appendQueryItems(name: String, values: [String]?, to items: inout [URLQueryItem]) {
            guard let values, !values.isEmpty else {
                return
            }

            let joined = values.joined(separator: ",")
            items.append(URLQueryItem(name: name, value: joined))
        }

        private static func appendQueryItem(name: String, value: Bool?, to items: inout [URLQueryItem]) {
            guard let value else {
                return
            }

            items.append(URLQueryItem(name: name, value: value ? "true" : "false"))
        }

        private static func appendQueryItem(name: String, value: Bool, to items: inout [URLQueryItem]) {
            items.append(URLQueryItem(name: name, value: value ? "true" : "false"))
        }
    }

    struct Endpoint: URLConvertible {
        let baseUrl: URL
        let path: String
        let queryItems: [URLQueryItem]
        let httpMethod: String
        let params: Encodable?

        init(
            baseUrl: URL,
            path: String,
            queryItems: [URLQueryItem],
            httpMethod: String = HttpMethod.get.rawValue,
            params: Encodable? = nil
        ) {
            self.baseUrl = baseUrl
            self.path = path
            self.queryItems = queryItems
            self.httpMethod = httpMethod
            self.params = params
        }

        var url: URL {
            var components = URLComponents(
                url: baseUrl.appendingPathComponent(path),
                resolvingAgainstBaseURL: false
            )
            components?.queryItems = queryItems.isEmpty ? nil : queryItems

            guard let url = components?.url else {
                assertionFailure()
                return baseUrl
            }

            return url
        }
    }

    struct ServiceProviderFilters: Hashable {
        var serviceProviders: [String]?
        var statuses: [String]?
        var categories: [String]?
        var accountFilter: Bool?
        var countries: [String]?
        var fiatCurrencies: [String]?
        var cryptoChains: [String]?
        var cryptoCurrencies: [String]?

        init(
            serviceProviders: [String]? = nil,
            statuses: [String]? = nil,
            categories: [String]? = nil,
            accountFilter: Bool? = nil,
            countries: [String]? = nil,
            fiatCurrencies: [String]? = nil,
            cryptoChains: [String]? = nil,
            cryptoCurrencies: [String]? = nil
        ) {
            self.serviceProviders = serviceProviders
            self.statuses = statuses
            self.categories = categories
            self.accountFilter = accountFilter
            self.countries = countries
            self.fiatCurrencies = fiatCurrencies
            self.cryptoChains = cryptoChains
            self.cryptoCurrencies = cryptoCurrencies
        }

        func applying<T>(
            _ keyPath: WritableKeyPath<ServiceProviderFilters, T>,
            _ value: T
        ) -> ServiceProviderFilters {
            var copy = self
            copy[keyPath: keyPath] = value
            return copy
        }
    }
}
