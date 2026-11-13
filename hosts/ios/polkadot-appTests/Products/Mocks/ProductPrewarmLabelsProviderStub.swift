import Foundation

@testable import polkadot_app

@MainActor
final class ProductPrewarmLabelsProviderStub: ProductPrewarmLabelsProviding {
    private(set) var lookupCount = 0

    let changes = StateChangeSignal()

    private let stubbedLabels: [String]

    init(labels: [String] = []) {
        stubbedLabels = labels
    }

    func labels() async -> [String] {
        lookupCount += 1
        changes.signal()
        return stubbedLabels
    }
}
