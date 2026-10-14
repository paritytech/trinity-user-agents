package io.paritytech.polkadotapp.chains.network.binding

import io.paritytech.polkadotapp.common.data.substrate.model.bindAccountIdentifier
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId

fun bindAccountId(decoded: Any?): AccountId = bindAccountIdentifier(decoded).intoAccountId()
