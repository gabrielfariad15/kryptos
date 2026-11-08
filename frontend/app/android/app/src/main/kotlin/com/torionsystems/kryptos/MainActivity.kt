package com.torionsystems.kryptos

import android.content.pm.ApplicationInfo
import android.os.Bundle
import android.view.WindowManager
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // Tela segura no app inteiro (aprovado em 07/11/2026): sem print, sem gravacao
        // de tela e sem miniatura na lista de apps recentes. Antes do super.onCreate: a
        // janela ja existe e nenhum quadro do Flutter e desenhado sem a protecao.
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        super.onCreate(savedInstanceState)
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, ChaveDoAparelho.CANAL)
            .setMethodCallHandler(ChaveDoAparelho(applicationContext))
        // "segura": o teste no aparelho confere a protecao na janela de verdade.
        // "liberar": tira a protecao para filmar e capturar no teste. So em app de
        // debug; no de producao a tela segura nao sai.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CANAL_TELA)
            .setMethodCallHandler { chamada, resposta ->
                when (chamada.method) {
                    "segura" -> resposta.success(
                        (window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE) != 0
                    )
                    "liberar" -> {
                        val debug = (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
                        if (debug) {
                            window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
                            resposta.success(null)
                        } else {
                            resposta.error("TELA_SEGURA_OBRIGATORIA", "so em app de debug", null)
                        }
                    }
                    else -> resposta.notImplemented()
                }
            }
    }

    companion object {
        const val CANAL_TELA = "kryptos/tela"
    }
}
