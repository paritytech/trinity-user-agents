import Foundation
import Products
import SubstrateSdk

enum PolkadotHostSigningModel {
    case signingRequest(PolkadotHostRemoteMessage.SigningRequest)
    case createTransaction(CreateTransactionPayload<ProductAccountId>)

    case legacyRawPayload(account: AccountId, type: PolkadotHostRemoteMessage.SigningRawPayload.PayloadType)
    case legacyCreateTransaction(payload: CreateTransactionPayload<LegacyAccountId>)
    case legacySignPayload(payload: SignTransactionPayload<SS58Account>)
}
