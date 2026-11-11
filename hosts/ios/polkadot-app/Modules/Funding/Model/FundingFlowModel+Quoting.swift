import Foundation
import TrUAPIHost

extension FundingFlowModel {
    /// Returns once the quote waiting on the keypad pause, every core call
    /// made so far and every brand read have finished.
    func settled() async {
        await quotes.pauseSettled()
        await coreCalls.idle()
        await brands.settled()
    }

    /// The core changed the quote list: show it when it answers the latest
    /// request, or none once the core withdrew it.
    func refreshQuotes() {
        guard let list = service.quotes(intent: intent) else {
            quotes.withdrawn()
            return
        }

        receive(quotes: list)
    }

    /// Takes the core's quote list when it answers the latest ask. The core
    /// keeps refreshing the last ask after a rail change drops it, and a list
    /// can arrive before the core's id for its ask does.
    func receive(quotes list: FundingQuoteList) {
        quotes.receive(list)
    }
}
