import Foundation
import PolkadotUI
import UIKitExt

protocol FiatOnRampProviderViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModels: [FiatOnRampProviderItemViewModel])
    func didReceive(isLoading: Bool)
    func didReceive(isWidgetLoading: Bool)
    func didReceive(isRefreshing: Bool)
    func didReceive(refreshCountdownText: String?)
    func didReceive(confirmUrl: URL)
}

@MainActor
protocol FiatOnRampProviderPresenterProtocol: AnyObject {
    func setup()
    func select(provider: FiatOnRampProviderItemViewModel)
    func openWidget(url: URL)
}

protocol FiatOnRampProviderInteractorInputProtocol: AnyObject {
    func setup()
    func select(providerId: String)
    func refreshQuotes()
    func track(sessionId: FiatOnRampSessionId)
}

@MainActor
protocol FiatOnRampProviderInteractorOutputProtocol: AnyObject {
    func didReceive(providers: [FiatOnrampProviderSummary], quotes: [FiatOnrampQuoteSummary])
    func didReceive(widgetSession: FiatOnrampWidgetSessionResponse)
    func didFailWidgetSession()
}

@MainActor
protocol FiatOnRampProviderWireframeProtocol: AnyObject {
    func showWidget(url: URL, from view: FiatOnRampProviderViewProtocol?)
}
