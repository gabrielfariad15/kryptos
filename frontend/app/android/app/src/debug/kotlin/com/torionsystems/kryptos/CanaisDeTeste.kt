package com.torionsystems.kryptos

import android.app.Activity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/**
 * Canais que só existem no app de debug: hoje, a sonda BLE (modo-direto.md, passo 1).
 * O profile e o release têm um CanaisDeTeste vazio (src/profile e src/release), e o app de
 * distribuição não leva o código da sonda nem as permissões de Bluetooth.
 */
object CanaisDeTeste {
    private var sonda: SondaBle? = null

    fun registrar(activity: Activity, engine: FlutterEngine) {
        sonda?.encerrar()
        val canal = MethodChannel(engine.dartExecutor.binaryMessenger, SondaBle.CANAL)
        val s = SondaBle(activity, canal)
        canal.setMethodCallHandler(s)
        sonda = s
    }

    /** O app saiu da frente: a sonda para tudo (decisão 3 do modo-direto.md). */
    fun aoPausar() {
        sonda?.pararDeFora("APP_SAIU_DA_FRENTE")
    }

    fun aoDestruir() {
        sonda?.encerrar()
        sonda = null
    }

    fun aoResultadoDePermissao(codigo: Int) {
        sonda?.aoResultadoDePermissao(codigo)
    }
}
