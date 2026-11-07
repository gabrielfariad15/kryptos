package com.torionsystems.kryptos

import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import android.util.Log
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.security.KeyStore
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import javax.crypto.KeyGenerator
import javax.crypto.Mac
import javax.crypto.SecretKey
import javax.crypto.SecretKeyFactory

/**
 * Chave HMAC do aparelho, no Keystore do Android. O material da chave não sai do
 * hardware seguro: o app só manda dados para o HMAC e recebe o resultado.
 *
 * É a segunda etapa do cofre da identidade (documentacao/arquitetura/cofre-da-identidade.md):
 * o núcleo estica o PIN, esta classe passa o resultado no HMAC, o núcleo abre o cofre.
 *
 * Nada aqui vai para log além do tipo do erro (regra 8.2). O trabalho roda fora da
 * thread principal: gerar chave no StrongBox pode levar segundos.
 */
class ChaveDoAparelho(private val contexto: Context) : MethodChannel.MethodCallHandler {

    companion object {
        const val CANAL = "kryptos/chave-do-aparelho"
        private const val PROVEDOR = "AndroidKeyStore"
        private const val TAG = "Kryptos"
    }

    private val trabalho: ExecutorService = Executors.newSingleThreadExecutor()
    private val principal = Handler(Looper.getMainLooper())

    private class ErroDaChave(val codigo: String) : Exception(codigo)

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        trabalho.execute {
            try {
                val resposta: Any? = when (call.method) {
                    "pastas" -> mapOf(
                        "semBackup" to contexto.noBackupFilesDir.absolutePath,
                        "dados" to contexto.dataDir.absolutePath,
                    )
                    "criar" -> criar(alias(call))
                    "hmac" -> hmac(alias(call), dados(call))
                    "existe" -> keystore().containsAlias(alias(call))
                    "nivel" -> nivel(alias(call))
                    "apagar" -> {
                        keystore().deleteEntry(alias(call))
                        null
                    }
                    else -> {
                        principal.post { result.notImplemented() }
                        return@execute
                    }
                }
                principal.post { result.success(resposta) }
            } catch (e: ErroDaChave) {
                principal.post { result.error(e.codigo, e.codigo, null) }
            } catch (e: Exception) {
                // Só o tipo: a mensagem de exceções do Keystore pode trazer detalhes do aparelho.
                Log.e(TAG, "chave do aparelho: ${call.method} falhou (${e.javaClass.simpleName})")
                principal.post { result.error("KEYSTORE", e.javaClass.simpleName, null) }
            }
        }
    }

    private fun alias(call: MethodCall): String =
        call.argument<String>("alias")?.takeIf { it.isNotBlank() } ?: throw ErroDaChave("ALIAS_AUSENTE")

    private fun dados(call: MethodCall): ByteArray =
        call.argument<ByteArray>("dados")?.takeIf { it.isNotEmpty() } ?: throw ErroDaChave("DADOS_AUSENTES")

    private fun keystore(): KeyStore = KeyStore.getInstance(PROVEDOR).apply { load(null) }

    /** Troca a chave do alias por uma nova. Tenta o StrongBox; sem ele, o TEE. */
    private fun criar(alias: String): String {
        val ks = keystore()
        if (ks.containsAlias(alias)) ks.deleteEntry(alias)
        try {
            gerar(alias, strongBox = true)
        } catch (e: StrongBoxUnavailableException) {
            gerar(alias, strongBox = false)
        }
        return nivel(alias)
    }

    private fun gerar(alias: String, strongBox: Boolean) {
        val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
            .setIsStrongBoxBacked(strongBox)
            .build()
        val gerador = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_HMAC_SHA256, PROVEDOR)
        gerador.init(spec)
        gerador.generateKey()
    }

    private fun chave(alias: String): SecretKey =
        keystore().getKey(alias, null) as? SecretKey ?: throw ErroDaChave("CHAVE_AUSENTE")

    private fun hmac(alias: String, dados: ByteArray): ByteArray {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(chave(alias))
        return mac.doFinal(dados)
    }

    /** Onde a chave ficou de fato: STRONGBOX, TEE, SOFTWARE (ou HARDWARE antes do Android 12). */
    private fun nivel(alias: String): String {
        val k = chave(alias)
        val info = SecretKeyFactory.getInstance(k.algorithm, PROVEDOR)
            .getKeySpec(k, KeyInfo::class.java) as KeyInfo
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            when (info.securityLevel) {
                KeyProperties.SECURITY_LEVEL_STRONGBOX -> "STRONGBOX"
                KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> "TEE"
                KeyProperties.SECURITY_LEVEL_SOFTWARE -> "SOFTWARE"
                else -> "DESCONHECIDO"
            }
        } else {
            @Suppress("DEPRECATION")
            if (info.isInsideSecureHardware) "HARDWARE" else "SOFTWARE"
        }
    }
}
