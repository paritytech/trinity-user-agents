import AsyncExtensions
import Foundation
import SubstrateSdk
import BigInt

public struct PaymentBalance: Encodable {
    @StringCodable public var available: Balance

    public init(available: Balance) {
        self.available = available
    }
}

/// Payment status as seen by product scripts. `completed`, `partiallyClaimed` and `failed` are terminal.
public enum HostPaymentStatus: Sendable, Equatable {
    case processing
    case completed
    /// Money moved, but less than requested: `settledInPlanks` reached the destination, the rest never will.
    case partiallyClaimed(settledInPlanks: Balance)
    case failed(reason: String)
}

/// Coded `paymentRequest` errors. Messages keep the legacy strings the current
/// container.js matches on. Anything else the host throws reaches the product uncoded.
public enum HostPaymentRequestError: Error, Hashable {
    case rejected
    case insufficientBalance
    case alreadyExists

    public var code: String {
        switch self {
        case .rejected: "Rejected"
        case .insufficientBalance: "InsufficientBalance"
        case .alreadyExists: "AlreadyExists"
        }
    }

    public var message: String {
        switch self {
        case .rejected: "payment rejected"
        case .insufficientBalance: "insufficient balance"
        case .alreadyExists: "A payment for the given id already exists"
        }
    }
}

extension HostPaymentRequestError: HostCallCodedError {}

/// Typed error for `host_payment_status_subscribe`, serialized to JS via `HostCallCodedError`.
public enum HostPaymentStatusError: HostCallCodedError {
    case notFound

    public var code: String { "NotFound" }
    public var message: String { "payment not found" }
}

// MARK: - Wire DTOs

/// Decoded request for `paymentRequest`. Byte fields are hex strings on the wire; the id is opaque
/// to the host, so its length is not validated here (same as `paymentTopUp`).
public struct PaymentRequestDto: Decodable {
    @HexCodable public var id: PaymentRequestId
    @StringCodable public var amount: Balance
    @HexCodable public var destination: AccountId

    private enum CodingKeys: String, CodingKey {
        case id = "idHex"
        case amount
        case destination = "destinationHex"
    }
}

/// Decoded request for `paymentStatusSubscribe`.
public struct PaymentStatusSubscribeDto: Decodable {
    @HexCodable public var id: PaymentRequestId

    private enum CodingKeys: String, CodingKey {
        case id = "idHex"
    }
}

/// Wire representation of ``HostPaymentStatus``: a tagged struct encoded via `toScaleCompatibleJSON()`.
public struct HostPaymentStatusDto: Encodable {
    public let tag: String
    public let value: String?

    public init(status: HostPaymentStatus) {
        switch status {
        case .processing:
            tag = "Processing"
            value = nil
        case .completed:
            tag = "Completed"
            value = nil
        case let .partiallyClaimed(settledInPlanks):
            tag = "PartiallyClaimed"
            value = String(settledInPlanks)
        case let .failed(reason):
            tag = "Failed"
            value = reason
        }
    }
}

/// Top-up status as seen by product scripts. Mirrors `host_payment_top_up_status`. Terminal:
/// `claimed(finalized: true)`, `claimedPartially`, `notClaimed`.
public enum HostPaymentTopUpStatus: Sendable, Equatable {
    case detecting
    case claiming
    case claimed(finalized: Bool)
    case claimedPartially(actualClaimed: Balance)
    case notClaimed
}

/// Typed error for `host_payment_top_up`, serialized to JS via `HostCallCodedError` so product
/// scripts can reconstruct the `PaymentTopUpErr` variant.
public enum HostPaymentTopUpError: HostCallCodedError {
    case invalidSource
    case alreadyExists
    case sourceBusy
    case notFound(String)
    case unknown(reason: String)

    public var code: String {
        switch self {
        case .invalidSource: "InvalidSource"
        case .alreadyExists: "AlreadyExists"
        case .sourceBusy: "SourceBusy"
        case .notFound: "NotFound"
        case .unknown: "Unknown"
        }
    }

    public var message: String {
        switch self {
        case .invalidSource: "The source account was not found or is invalid"
        case .alreadyExists: "A top up for the given id already exists"
        case .sourceBusy: "The source is already used by another active top up"
        case let .notFound(paymentId): "Top up with given payment id not found \(paymentId)"
        case let .unknown(reason): reason
        }
    }
}

// MARK: - Wire DTOs

/// Decoded request for `paymentTopUp`. `amount`/`id` sit alongside the flat `sourceTag`/`sourceKey…`
/// fields, so `source` is decoded from the same container.
public struct PaymentTopUpRequestDto: Decodable {
    @StringCodable public var amount: Balance
    @HexCodable public var id: PaymentTopUpId
    public let source: PaymentTopUpSource

    private enum CodingKeys: String, CodingKey {
        case amount
        case id
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        _amount = try container.decode(StringCodable<Balance>.self, forKey: .amount)
        _id = try container.decode(HexCodable<PaymentTopUpId>.self, forKey: .id)
        // The source fields are flat siblings of `amount`/`id`, not a nested object, so `source`
        // decodes from the top-level decoder rather than from a `source` key.
        source = try PaymentTopUpSource(from: decoder)
    }
}

/// Decoded request for `paymentTopUpStatusSubscribe`.
public struct PaymentTopUpStatusSubscribeDto: Decodable {
    @HexCodable public var id: PaymentTopUpId
}

/// Wire representation of ``HostPaymentTopUpStatus`` — a tagged struct (`tag` + the fields the tag
/// carries), encoded to the bridge via `toScaleCompatibleJSON()`.
public struct HostPaymentTopUpStatusDto: Encodable {
    public let tag: String
    public let finalized: Bool?
    @OptionStringCodable public var actualClaimed: Balance?

    public init(status: HostPaymentTopUpStatus) {
        switch status {
        case .detecting:
            tag = "Detecting"
            finalized = nil
            _actualClaimed = OptionStringCodable(wrappedValue: nil)
        case .claiming:
            tag = "Claiming"
            finalized = nil
            _actualClaimed = OptionStringCodable(wrappedValue: nil)
        case let .claimed(finalized):
            tag = "Claimed"
            self.finalized = finalized
            _actualClaimed = OptionStringCodable(wrappedValue: nil)
        case let .claimedPartially(actualClaimed):
            tag = "ClaimedPartially"
            finalized = nil
            _actualClaimed = OptionStringCodable(wrappedValue: actualClaimed)
        case .notClaimed:
            tag = "NotClaimed"
            finalized = nil
            _actualClaimed = OptionStringCodable(wrappedValue: nil)
        }
    }
}
