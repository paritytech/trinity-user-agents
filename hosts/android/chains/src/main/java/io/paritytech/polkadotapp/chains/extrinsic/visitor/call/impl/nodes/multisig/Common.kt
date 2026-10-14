package io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.nodes.multisig

import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.novasama.substrate_sdk_android.runtime.definitions.types.useScaleWriter
import io.novasama.substrate_sdk_android.scale.utils.directWrite
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.compareTo
import io.paritytech.polkadotapp.common.utils.padEnd

private val PREFIX = "modlpy/utilisuba".encodeToByteArray()

fun generateMultisigAddress(
    signatory: AccountId,
    otherSignatories: List<AccountId>,
    threshold: Int
) = generateMultisigAddress(otherSignatories + signatory, threshold)

fun generateMultisigAddress(
    signatories: List<AccountId>,
    threshold: Int
): AccountId {
    val accountIdSize = signatories.first().value.size

    val sortedAccounts = signatories.sortedWith { a, b -> a.value.compareTo(b.value, unsigned = true) }

    val entropy = useScaleWriter {
        directWrite(PREFIX)

        writeCompact(sortedAccounts.size)
        sortedAccounts.forEach {
            directWrite(it.value)
        }

        writeUint16(threshold)
    }.blake2b256()

    val result = when {
        entropy.size == accountIdSize -> entropy
        entropy.size < accountIdSize -> entropy.padEnd(accountIdSize, 0)
        else -> entropy.copyOf(accountIdSize)
    }

    return result.intoAccountId()
}
