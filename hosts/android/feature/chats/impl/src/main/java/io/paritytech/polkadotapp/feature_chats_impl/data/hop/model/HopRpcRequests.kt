package io.paritytech.polkadotapp.feature_chats_impl.data.hop.model

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

class HopSubmitRequest(
    data: String,
    recipients: List<String>,
    signature: String,
    signer: String,
    submitTimestamp: Long
) : RuntimeRequest(
    method = "hop_submit",
    params = listOf(data, recipients, signature, signer, submitTimestamp)
)

class HopClaimRequest(
    hash: String,
    signature: String
) : RuntimeRequest(
    method = "hop_claim",
    params = listOf(hash, signature)
)

class HopAckRequest(
    hash: String,
    signature: String
) : RuntimeRequest(
    method = "hop_ack",
    params = listOf(hash, signature)
)

class BitswapGetRequest(
    cid: String
) : RuntimeRequest(
    method = "bitswap_v1_get",
    params = listOf(cid)
)
