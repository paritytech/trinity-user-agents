package io.paritytech.polkadotapp.feature_coinage_api.domain.submitter

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import java.math.BigDecimal

/**
 * A pluggable destination for a coinage transfer memo. Features contribute implementations keyed by
 * a [submitterId] via Dagger `@IntoMap`; the send flow looks one up and delegates, so it stays
 * agnostic of how/where the coins are delivered (chat, statement store, …).
 *
 * The [submitterPayload] is opaque to the send flow — each submitter defines and decodes its own
 * payload (e.g. an encrypted-statement topic + merchant key).
 */
interface CoinsSubmitter {
    suspend fun submit(memo: TransferMemo, amount: BigDecimal, submitterPayload: ByteArray): Result<Unit>
}
