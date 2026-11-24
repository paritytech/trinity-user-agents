package io.parity.qjsbench

/** The engines in libqjs_benchkit.so: native QuickJS, wasm2c, wasmi and Boa. */
object BenchKit {
    init {
        System.loadLibrary("qjs_benchkit")
    }

    /** One measurement on a fresh thread; returns its JSON line. */
    external fun run(engine: String, mode: String, resources: String, count: Int): String

    /** The wasm2c variant the library was built with. */
    external fun variant(): String
}
