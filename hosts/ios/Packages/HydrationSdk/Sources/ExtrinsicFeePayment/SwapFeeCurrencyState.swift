import Foundation
import SubstrateSdk
import SubstrateStorageSubscription
import Foundation_iOS
import CommonService

public extension HydraDx {
    struct SwapFeeCurrencyState: ObservableSubscriptionStateProtocol {
        public typealias TChange = SwapFeeCurrencyStateChange

        public let feeCurrency: HydraDx.AssetId?

        init(feeCurrency: HydraDx.AssetId?) {
            self.feeCurrency = feeCurrency
        }

        public init(change: HydraDx.SwapFeeCurrencyStateChange) {
            feeCurrency = change.feeCurrency.valueWhenDefined(else: nil)
        }

        public func merging(change: SwapFeeCurrencyStateChange) -> SwapFeeCurrencyState {
            .init(feeCurrency: change.feeCurrency.valueWhenDefined(else: feeCurrency))
        }
    }

    struct SwapFeeCurrencyStateChange: BatchStorageSubscriptionResult {
        enum Key: String {
            case feeCurrency
        }

        public let feeCurrency: UncertainStorage<HydraDx.AssetId?>

        public init(
            values: [BatchStorageSubscriptionResultValue],
            blockHashJson _: JSON,
            context: [CodingUserInfoKey: Any]?
        ) throws {
            feeCurrency = try UncertainStorage<StringScaleMapper<HydraDx.AssetId>?>(
                values: values,
                mappingKey: Key.feeCurrency.rawValue,
                context: context
            ).map { $0?.value }
        }
    }
}
