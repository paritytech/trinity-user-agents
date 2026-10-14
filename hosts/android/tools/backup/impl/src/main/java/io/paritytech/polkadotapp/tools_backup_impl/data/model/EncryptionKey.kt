package io.paritytech.polkadotapp.tools_backup_impl.data.model

import io.novasama.substrate_sdk_android.extensions.toHexString

@JvmInline
value class EncryptionKey(val value: ByteArray)

fun EncryptionKey.toHex() = value.toHexString()
