package io.paritytech.polkadotapp.design.components.mnemonic.model

data class Word(
    val index: Int,
    val value: String
)

fun List<String>.toWordList() = mapIndexed { index, string -> Word(index + 1, string) }
