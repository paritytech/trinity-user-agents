import Observation
import UIKit

public struct FiatOnRampProviderItemViewModel: Identifiable {
    public let id: String
    public let name: String
    public let icon: ImageViewModelProtocol?
    public let quoteText: String?
    public let fiatAmountText: String?

    public init(
        id: String,
        name: String,
        icon: ImageViewModelProtocol?,
        quoteText: String?,
        fiatAmountText: String?
    ) {
        self.id = id
        self.name = name
        self.icon = icon
        self.quoteText = quoteText
        self.fiatAmountText = fiatAmountText
    }
}

public struct FiatOnRampProviderConfirmation: Identifiable {
    public let id = UUID()
    public let url: URL

    public init(url: URL) {
        self.url = url
    }
}

public protocol FiatOnRampProviderViewModelProtocol: Observation.Observable {
    var viewModels: [FiatOnRampProviderItemViewModel] { get set }
    var isLoading: Bool { get set }
    var isWidgetLoading: Bool { get set }
    var isRefreshing: Bool { get set }
    var refreshCountdownText: String? { get set }
    var confirmation: FiatOnRampProviderConfirmation? { get set }
    var onConfirmOpenUrl: ((URL) -> Void)? { get set }
    var onSelect: ((FiatOnRampProviderItemViewModel) -> Void)? { get set }
}

@Observable
public final class FiatOnRampProviderViewModel: FiatOnRampProviderViewModelProtocol {
    public var viewModels: [FiatOnRampProviderItemViewModel] = []
    public var isLoading: Bool = false
    public var isWidgetLoading: Bool = false
    public var isRefreshing: Bool = false
    public var refreshCountdownText: String?
    public var confirmation: FiatOnRampProviderConfirmation?
    public var onConfirmOpenUrl: ((URL) -> Void)?
    public var onSelect: ((FiatOnRampProviderItemViewModel) -> Void)?

    public init() {}
}
