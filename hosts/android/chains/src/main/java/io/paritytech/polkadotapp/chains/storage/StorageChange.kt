package io.paritytech.polkadotapp.chains.storage

data class StorageChange(val block: String, val key: String, val value: String?)
