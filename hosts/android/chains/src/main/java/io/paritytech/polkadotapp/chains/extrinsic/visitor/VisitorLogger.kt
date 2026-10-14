package io.paritytech.polkadotapp.chains.extrinsic.visitor

import timber.log.Timber

internal interface ExtrinsicVisitorLogger {
    fun info(message: String)

    fun error(message: String)
}

internal class IndentVisitorLogger(
    private val tag: String = "ExtrinsicVisitor",
    private val indent: Int = 0
) : ExtrinsicVisitorLogger {
    private val timber = Timber.tag(tag)
    private val indentPrefix = " ".repeat(indent)

    override fun info(message: String) {
        timber.d("%s%s", indentPrefix, message)
    }

    override fun error(message: String) {
        timber.e("%s%s", indentPrefix, message)
    }
}
