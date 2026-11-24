package io.paritytech.polkadotapp.feature_products_impl.domain.funding

/** A funding session, by the intent the core gave it. */
@JvmInline
value class FundingIntentId(val value: String)

/** One provider screen the core presents for a session. */
@JvmInline
value class FundingFrameRequestId(val value: String)
