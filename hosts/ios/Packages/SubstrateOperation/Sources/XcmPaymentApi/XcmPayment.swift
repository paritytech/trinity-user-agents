import Foundation
import SubstrateSdk

public enum XcmPayment {
    public static let apiName = "XcmPaymentApi"

    public typealias WeightResult = Substrate.Result<Substrate.WeightV2, JSON>
}
