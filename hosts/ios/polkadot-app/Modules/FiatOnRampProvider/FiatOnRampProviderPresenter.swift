import Foundation
import Foundation_iOS
import PolkadotUI

@MainActor
final class FiatOnRampProviderPresenter {
    weak var view: FiatOnRampProviderViewProtocol?
    let interactor: FiatOnRampProviderInteractorInputProtocol
    let wireframe: FiatOnRampProviderWireframeProtocol
    private lazy var quoteFormatter: NumberFormatter = {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.minimumFractionDigits = 0
        formatter.maximumFractionDigits = 6
        formatter.locale = Locale.autoupdatingCurrent
        return formatter
    }()

    private let formatterFactory: AssetBalanceFormatterFactoryProtocol = AssetBalanceFormatterFactory()
    private lazy var priceFormatter: LocalizableDecimalFormatting = formatterFactory
        .createAssetPriceFormatter(for: .usd).value(for: .current)

    private var refreshCountdown: Int = Constants.refreshInterval
    private var refreshTimer: Timer?
    private var hasProviders: Bool = false
    private var isWidgetLoading: Bool = false
    private var widgetSession: FiatOnrampWidgetSessionResponse?

    deinit {
        refreshTimer?.invalidate()
    }

    init(
        interactor: FiatOnRampProviderInteractorInputProtocol,
        wireframe: FiatOnRampProviderWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension FiatOnRampProviderPresenter: FiatOnRampProviderPresenterProtocol {
    func setup() {
        view?.didReceive(isLoading: true)
        view?.didReceive(isWidgetLoading: false)
        startRefreshTimer()
        interactor.setup()
    }

    func select(provider: FiatOnRampProviderItemViewModel) {
        isWidgetLoading = true
        view?.didReceive(isWidgetLoading: true)
        view?.didReceive(isRefreshing: false)
        interactor.select(providerId: provider.id)
    }

    func openWidget(url _: URL) {
        guard let widgetSession else {
            return
        }

        wireframe.showWidget(url: widgetSession.widgetUrl, from: view)
        interactor.track(sessionId: widgetSession.sessionId)
    }
}

private extension FiatOnRampProviderPresenter {
    enum Constants {
        static let refreshInterval = 30
    }

    func startRefreshTimer() {
        refreshCountdown = Constants.refreshInterval
        updateRefreshCountdownText()

        refreshTimer?.invalidate()
        refreshTimer = Timer.scheduledTimer(
            withTimeInterval: 1,
            repeats: true
        ) { [weak self] _ in
            self?.handleRefreshTick()
        }
    }

    func handleRefreshTick() {
        if refreshCountdown <= 1 {
            if !isWidgetLoading {
                if hasProviders {
                    view?.didReceive(isRefreshing: true)
                }
                interactor.refreshQuotes()
            }
            refreshCountdown = Constants.refreshInterval
        } else {
            refreshCountdown -= 1
        }

        updateRefreshCountdownText()
    }

    func updateRefreshCountdownText() {
        let minutes = refreshCountdown / 60
        let seconds = refreshCountdown % 60
        let timeText = String(format: "%d:%02d", minutes, seconds)
        let title = String(localized: .fiatOnrampProvidersRefresh(timeText))
        view?.didReceive(refreshCountdownText: title)
    }
}

extension FiatOnRampProviderPresenter: FiatOnRampProviderInteractorOutputProtocol {
    func didReceive(providers: [FiatOnrampProviderSummary], quotes: [FiatOnrampQuoteSummary]) {
        hasProviders = !providers.isEmpty

        let quotesByProvider = quotes.reduce(into: [String: FiatOnrampQuoteSummary]()) { result, quote in
            result[quote.providerId] = quote
        }

        let providersWithQuotes = providers.compactMap { provider -> (
            FiatOnrampProviderSummary,
            FiatOnrampQuoteSummary
        )? in
            guard let quote = quotesByProvider[provider.id] else {
                return nil
            }
            return (provider, quote)
        }

        let viewModels = providersWithQuotes.map { provider, quote in
            let amount = formatQuoteAmount(quote.cryptoAmount)
            let quoteText = "\(amount) \(quote.cryptoCurrencyCode)"
            let fiatText = priceFormatter.stringFromDecimal(quote.fiatAmount)

            return FiatOnRampProviderItemViewModel(
                id: provider.id,
                name: provider.name,
                icon: RemoteImageViewModel(url: provider.iconUrl),
                quoteText: quoteText,
                fiatAmountText: fiatText
            )
        }

        if !isWidgetLoading {
            view?.didReceive(isLoading: false)
        }
        view?.didReceive(isRefreshing: false)
        view?.didReceive(viewModels: viewModels)
    }

    func didReceive(widgetSession: FiatOnrampWidgetSessionResponse) {
        self.widgetSession = widgetSession

        isWidgetLoading = false
        view?.didReceive(isWidgetLoading: false)

        view?.didReceive(confirmUrl: widgetSession.widgetUrl)
    }

    func didFailWidgetSession() {
        isWidgetLoading = false
        view?.didReceive(isWidgetLoading: false)
    }
}

private extension FiatOnRampProviderPresenter {
    func formatQuoteAmount(_ amount: Decimal) -> String {
        let number = NSDecimalNumber(decimal: amount)
        if let formatted = quoteFormatter.string(from: number) {
            return formatted
        }
        return number.stringValue
    }
}
