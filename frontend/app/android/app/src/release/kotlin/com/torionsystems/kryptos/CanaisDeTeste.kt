package com.torionsystems.kryptos

import android.app.Activity
import io.flutter.embedding.engine.FlutterEngine

/** App de distribuição: nenhum canal de teste. O de debug fica em src/debug. */
object CanaisDeTeste {
    @Suppress("UNUSED_PARAMETER")
    fun registrar(activity: Activity, engine: FlutterEngine) = Unit

    fun aoPausar() = Unit

    fun aoDestruir() = Unit

    @Suppress("UNUSED_PARAMETER")
    fun aoResultadoDePermissao(codigo: Int) = Unit
}
