import Foundation
import SubstrateSdk
import SDKLogger

final class AssetConversionEventParser {
    let logger: SDKLoggerProtocol

    init(logger: SDKLoggerProtocol) {
        self.logger = logger
    }

    func extractDeposit(from events: [Event], using codingFactory: RuntimeCoderFactoryProtocol) -> Balance? {
        guard let event = events.last else {
            return nil
        }

        do {
            let parsedEvent: AssetConversionPallet.SwapExecutedEvent = try ExtrinsicExtraction.getEventParams(
                from: event,
                context: codingFactory.createRuntimeJsonContext()
            )

            return parsedEvent.amountOut
        } catch {
            logger.error("Event parsing error: \(error)")
            return nil
        }
    }
}
