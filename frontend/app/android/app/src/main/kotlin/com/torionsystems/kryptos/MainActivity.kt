package com.torionsystems.kryptos

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
        // So para o teste no aparelho conferir a protecao na janela de verdade.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CANAL_TELA)
            .setMethodCallHandler { chamada, resposta ->
                when (chamada.method) {
                    "segura" -> resposta.success(
                        (window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE) != 0
                    )
                    else -> resposta.notImplemented()
                }
            }
    }

    companion object {
        const val CANAL_TELA = "kryptos/tela"
    }
}
