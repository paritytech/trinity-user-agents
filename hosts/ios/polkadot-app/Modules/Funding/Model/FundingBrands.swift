import Observation
import TrUAPIHost

/// The providers' brands one funding overlay has read, each read once.
@MainActor
@Observable
final class FundingBrands {
    private var loaded: [ProductId: FundingProviderBrand] = [:]

    private let service: FundingFlowServiceProtocol
    /// Brands being read, by provider.
    @ObservationIgnored private var reads: [ProductId: Task<Void, Never>] = [:]

    init(service: FundingFlowServiceProtocol) {
        self.service = service
    }

    /// The brand read for `providerId`, or a placeholder until it is.
    func brand(for providerId: ProductId) -> FundingProviderBrand {
        loaded[providerId] ?? .placeholder(for: providerId)
    }

    /// Reads the brand of every provider not read or being read yet.
    func load(_ providerIds: [ProductId]) {
        for providerId in providerIds where loaded[providerId] == nil && reads[providerId] == nil {
            reads[providerId] = Task { [service] in
                loaded[providerId] = await service.brand(for: providerId)
                reads[providerId] = nil
            }
        }
    }

    /// Returns once every brand read has finished.
    func settled() async {
        for read in reads.values {
            await read.value
        }
    }
}
