package io.paritytech.polkadotapp.common.domain.model

import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAccountId
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAddress

typealias AccountId = DataByteArray
typealias EncodedPublicKey = DataByteArray
typealias EncodedPrivateKey = DataByteArray

fun ByteArray.intoAccountId(): AccountId = AccountId(this)

fun AccountId.toSubstrateAddress(ss58Prefix: Short) = value.toAddress(ss58Prefix)

fun String.isValidSubstrateAddress(): Boolean = runCatching { toAccountId() }.isSuccess
