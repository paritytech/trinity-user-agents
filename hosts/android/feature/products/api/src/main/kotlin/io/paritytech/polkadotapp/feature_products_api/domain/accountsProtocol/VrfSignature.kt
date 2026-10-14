package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.common.domain.model.DataByteArray

/** One `append_message(label, value)` call against the merlin transcript being signed. */
class VrfTranscriptItem(val label: DataByteArray, val value: DataByteArray)

/** An sr25519 (schnorrkel) VRF signature: the 32-byte `VRFPreOut` and the 64-byte `VRFProof`. */
class VrfSignature(val preOutput: DataByteArray, val proof: DataByteArray)
