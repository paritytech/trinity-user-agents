package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.opengl.GLES20

object CoinageGlProgram {
    fun link(vertexSource: String, fragmentSource: String): Int {
        val vertexShader = compile(GLES20.GL_VERTEX_SHADER, vertexSource)
        val fragmentShader = try {
            compile(GLES20.GL_FRAGMENT_SHADER, fragmentSource)
        } catch (failure: IllegalStateException) {
            GLES20.glDeleteShader(vertexShader)
            throw failure
        }
        val program = GLES20.glCreateProgram()

        GLES20.glAttachShader(program, vertexShader)
        GLES20.glAttachShader(program, fragmentShader)
        GLES20.glLinkProgram(program)
        GLES20.glDeleteShader(vertexShader)
        GLES20.glDeleteShader(fragmentShader)

        val status = IntArray(1)
        GLES20.glGetProgramiv(program, GLES20.GL_LINK_STATUS, status, 0)

        if (status[0] == 0) {
            val log = GLES20.glGetProgramInfoLog(program)

            GLES20.glDeleteProgram(program)
            error("program did not link: $log")
        }

        return program
    }

    private fun compile(type: Int, source: String): Int {
        val shader = GLES20.glCreateShader(type)

        GLES20.glShaderSource(shader, source)
        GLES20.glCompileShader(shader)

        val status = IntArray(1)
        GLES20.glGetShaderiv(shader, GLES20.GL_COMPILE_STATUS, status, 0)

        if (status[0] == 0) {
            val log = GLES20.glGetShaderInfoLog(shader)

            GLES20.glDeleteShader(shader)
            error("shader did not compile: $log")
        }

        return shader
    }
}
