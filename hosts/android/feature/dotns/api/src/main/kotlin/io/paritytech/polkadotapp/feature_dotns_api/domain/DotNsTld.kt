package io.paritytech.polkadotapp.feature_dotns_api.domain

/**
 * A network's dotNS top-level domain (`dot`, `paseo`).
 *
 * Construction is restricted to [parse] so every instance is a single valid DNS label —
 * the value crosses the chain trust boundary and is interpolated into validation regexes.
 */
@JvmInline
value class DotNsTld private constructor(val value: String) {
    val suffix: String get() = ".$value"

    override fun toString(): String = value

    companion object {
        private val VALID_TLD = Regex("""[a-z0-9][a-z0-9-]{0,62}""")

        fun parse(value: String): DotNsTld? {
            return value.takeIf { VALID_TLD.matches(it) }?.let(::DotNsTld)
        }
    }
}
