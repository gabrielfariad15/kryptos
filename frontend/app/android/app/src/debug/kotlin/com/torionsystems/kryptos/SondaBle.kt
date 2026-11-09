package com.torionsystems.kryptos

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothGattServer
import android.bluetooth.BluetoothGattServerCallback
import android.bluetooth.BluetoothGattService
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.BluetoothServerSocket
import android.bluetooth.BluetoothSocket
import android.bluetooth.BluetoothStatusCodes
import android.bluetooth.le.AdvertiseData
import android.bluetooth.le.AdvertisingSet
import android.bluetooth.le.AdvertisingSetCallback
import android.bluetooth.le.AdvertisingSetParameters
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.ParcelUuid
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.Closeable
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.security.SecureRandom
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.UUID
import java.util.concurrent.Executors

/**
 * Sonda medida do modo Direto (documentacao/arquitetura/modo-direto.md, passo 1). Só no app
 * de debug. Sem protocolo do Kryptos: manda bytes aleatórios do tamanho da maior carta e
 * mede.
 *
 * Papéis:
 *  - SERVIDOR: abre o serviço GATT e anuncia em dois conjuntos, o legado (1M) e, se o
 *    aparelho tiver, o de longo alcance (PHY Coded, anúncio estendido).
 *  - CLIENTE: procura, conecta, pede MTU, ajusta PHY e prioridade, e faz n idas (cliente
 *    escreve sem resposta) e n voltas, cada uma conferida pelo SHA-256. A volta vai pelo
 *    caminho escolhido na tela (QuadroSonda): NOTIFICACAO (servidor notifica), ESCRITA
 *    (servidor vira cliente GATT do cliente no mesmo enlace e escreve sem resposta) ou
 *    L2CAP (canal L2CAP orientado a conexão, sem pareamento).
 *  - AMBOS: os dois papéis ao mesmo tempo. Uma espera aleatória antes de conectar evita a
 *    maioria das colisões; quando os dois conectam mesmo assim, o desempate é no OLA (o
 *    primeiro quadro do cliente, com um nonce novo por conexão): cada servidor compara o
 *    nonce do outro com o do próprio cliente (QuadroSonda.decidirOla) e só um cliente segue.
 *    Nada a mais no anúncio. O servidor não decide pelo endereço: os celulares trocam o
 *    endereço Bluetooth por privacidade, e a conexão que chega do outro vem com um endereço
 *    diferente do da conexão que este abriu (teste de 09/11/2026).
 *
 * Comandos do script (ferramentas/sonda-roteiro.ps1): o script grava uma linha em
 * files/sonda-comando.txt pelo run-as (só app de debug, só pelo cabo; nenhum outro app
 * escreve ali) e a sonda lê a cada 250 ms, executa e marca "comando" ao receber e
 * "comando-fim" ao terminar. Com o app fora da frente, o comando é recusado (decisão 3).
 * Roteiro de alcance: o script carrega distâncias e combinações antes de os celulares
 * saírem do cabo; em cada ponto a pessoa toca PRÓXIMO PONTO e a sonda roda as rodadas.
 *
 * O que vai para o log e para o arquivo de medidas: só códigos e números (regra 8.2). Nem o
 * endereço Bluetooth do outro aparelho, que é um identificador.
 *
 * Todo o trabalho roda numa thread própria (HandlerThread), uma coisa de cada vez: o Android
 * só aceita uma operação GATT pendente por conexão.
 */
@SuppressLint("MissingPermission") // as permissões são conferidas em permissoesConcedidas()
class SondaBle(private val activity: Activity, private val canal: MethodChannel) : MethodChannel.MethodCallHandler {

    companion object {
        const val CANAL = "kryptos/sonda-ble"
        const val CODIGO_PERMISSAO = 0x5B1E
        private const val TAG = "KryptosSonda"

        // Identificador fixo do serviço da sonda. O anúncio leva só ele.
        val UUID_SERVICO: UUID = UUID.fromString("c50611b3-2c85-484b-9ffd-2759f53442a9")
        val UUID_ENTRADA: UUID = UUID.fromString("afd54b5e-7086-4e42-be9b-1c555eed9570") // cliente -> servidor
        val UUID_SAIDA: UUID = UUID.fromString("127449fd-703e-4fc1-83b2-1794f64f8c76")   // servidor -> cliente
        val UUID_CCCD: UUID = UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")
        // Caminho de volta ESCRITA: serviço aberto pelo CLIENTE, onde o servidor escreve.
        // Não vai em anúncio nenhum; o servidor acha pela descoberta de serviços.
        val UUID_SERVICO_VOLTA: UUID = UUID.fromString("736a45bc-a4ca-4a24-9b86-09393ce0b4b3")
        val UUID_VOLTA_ENTRADA: UUID = UUID.fromString("4b092b30-6288-4af6-9a06-a526e34296a3")

        // A maior carta medida (transportes.md, texto de 64 KiB). Medida, não estimada.
        const val TAMANHO_CARTA = 66_594
        const val MTU_PEDIDO = 517

        const val TEMPO_PROCURA_MS = 30_000L
        const val TEMPO_CONEXAO_MS = 15_000L
        const val TEMPO_OPERACAO_MS = 10_000L
        const val TEMPO_PHY_MS = 3_000L
        const val TEMPO_TRANSFERENCIA_MS = 30_000L
        // Preparar a volta ESCRITA ou L2CAP: abrir o caminho e o outro lado responder.
        const val TEMPO_PREPARAR_MS = 15_000L
        val CAMINHOS = setOf("NOTIFICACAO", "ESCRITA", "L2CAP")
        // Limite do Android: 5 startScan em 30 s por app; a sexta fica muda, sem erro.
        const val PROCURAS_POR_JANELA = 5
        const val JANELA_PROCURAS_MS = 30_000L
        const val ESPERA_AMBOS_MAX_MS = 1_500
        const val TENTATIVAS_OCUPADO_MAX = 5_000
        const val ARQUIVO = "sonda-ble.txt"
        const val ARQUIVO_MAX_BYTES = 2L * 1024 * 1024
        // O texto vai num Intent; acima de ~1 MB o Android recusa (TransactionTooLarge).
        // Estimo uma sessão inteira em dezenas de KB (a de 08/11/2026 teve 12 KB).
        const val COMPARTILHAR_MAX_BYTES = 400_000L
        const val ARQUIVO_COMANDO = "sonda-comando.txt"
        const val COMANDO_MAX_BYTES = 1_024L
        const val INTERVALO_COMANDO_MS = 250L
        // Entre duas rodadas do roteiro: o mesmo intervalo do roteiro feito à mão
        // (5 procuras em 30 s), com folga.
        const val ESPERA_ENTRE_RODADAS_MS = 6_500L
        val ACOES = setOf("inventario", "anunciar", "procurar", "calibrar", "ambos", "parar", "l2cap-teste", "roteiro")

        val PERMISSOES = arrayOf(
            Manifest.permission.BLUETOOTH_SCAN,
            Manifest.permission.BLUETOOTH_ADVERTISE,
            Manifest.permission.BLUETOOTH_CONNECT,
        )
    }

    private val linha = HandlerThread("sonda-ble").apply { start() }
    private val h = Handler(linha.looper)
    private val principal = Handler(Looper.getMainLooper())
    private val sorteio = SecureRandom()
    private val contexto = activity.applicationContext
    private val gerente: BluetoothManager? = contexto.getSystemService(BluetoothManager::class.java)
    private val adaptador: BluetoothAdapter? = gerente?.adapter
    private var permissaoPendente: MethodChannel.Result? = null

    // ---------- arquivo de medidas ----------

    private val arquivo = File(activity.filesDir, ARQUIVO)
    private val formatoHora = DateTimeFormatter.ofPattern("HH:mm:ss.SSS", Locale.ROOT)

    /** Uma linha: logcat, arquivo e tela. Só na thread da sonda. */
    private fun registrar(texto: String) {
        val l = "${LocalTime.now().format(formatoHora)} $texto"
        Log.i(TAG, texto)
        try {
            if (arquivo.length() < ARQUIVO_MAX_BYTES) {
                FileOutputStream(arquivo, true).use { it.write((l + "\n").toByteArray(Charsets.UTF_8)) }
            } else {
                Log.e(TAG, "FALHA sonda-ble ARQUIVO_CHEIO")
            }
        } catch (e: Exception) {
            Log.e(TAG, "FALHA sonda-ble ARQUIVO ${e.javaClass.simpleName}")
        }
        principal.post { canal.invokeMethod("linha", l) }
    }

    private fun medida(evento: String, campos: String) = registrar("MEDIDA sonda-ble $evento $campos")

    private var ultimaFalha = ""

    private fun falha(codigo: String, campos: String = "") {
        ultimaFalha = codigo
        registrar("FALHA sonda-ble $codigo${if (campos.isEmpty()) "" else " $campos"}")
    }

    private fun ocupado(sim: Boolean) = principal.post { canal.invokeMethod("ocupado", sim) }

    // ---------- canal com o Dart ----------

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "permissoes" -> {
                // A tela da sonda abriu: só então a sonda passa a ler comandos.
                h.post { iniciarComandos() }
                pedirPermissoes(result)
            }
            "proximo" -> h.post { proximoPonto(); principal.post { result.success(null) } }
            "inventario" -> h.post {
                val t = inventario()
                principal.post { result.success(t) }
            }
            "anunciar" -> h.post { iniciarServidor("PAPEIS"); principal.post { result.success(null) } }
            "procurar", "ambos" -> {
                val cfg = Config(
                    modo = if (call.method == "ambos") "AMBOS" else "PAPEIS",
                    phy = call.argument<String>("phy") ?: "1M",
                    prioridade = call.argument<String>("prioridade") ?: "PADRAO",
                    distancia = call.argument<Int>("distancia") ?: 0,
                    repeticoes = call.argument<Int>("repeticoes") ?: 5,
                    calibrar = call.argument<Boolean>("calibrar") ?: false,
                    volta = call.argument<String>("volta") ?: "NOTIFICACAO",
                )
                if (!configValida(cfg)) {
                    result.error("ARGUMENTO", "configuracao fora do esperado", null)
                    return
                }
                h.post {
                    if (cfg.modo == "AMBOS") iniciarServidor("AMBOS")
                    iniciarCliente(cfg)
                    principal.post { result.success(null) }
                }
            }
            "parar" -> h.post { parar("PEDIDO"); principal.post { result.success(null) } }
            "compartilhar" -> h.post { compartilhar(result) }
            else -> result.notImplemented()
        }
    }

    private fun configValida(cfg: Config) =
        cfg.phy in setOf("1M", "2M", "CODED") && cfg.prioridade in setOf("PADRAO", "ALTA") &&
            cfg.repeticoes in 1..50 && cfg.distancia >= 0 && cfg.volta in CAMINHOS

    // ---------- comandos do script ----------

    private val arquivoComando = File(activity.filesDir, ARQUIVO_COMANDO)
    private var lendoComandos = false
    private val verComandoPeriodico = object : Runnable {
        override fun run() {
            verComando()
            h.postDelayed(this, INTERVALO_COMANDO_MS)
        }
    }
    // Comando do script que abriu a rodada de cliente em andamento, e como ela terminou.
    private var comandoCliente: String? = null
    private var desfechoCliente = "parou"
    private var codigoDesfecho = ""

    private fun iniciarComandos() {
        if (lendoComandos) return
        lendoComandos = true
        // Comando que sobrou de antes de abrir não vale: o script manda de novo.
        if (arquivoComando.exists() && !arquivoComando.delete()) Log.e(TAG, "FALHA sonda-ble COMANDO_ANTIGO")
        h.post(verComandoPeriodico)
    }

    private fun verComando() {
        if (!arquivoComando.exists()) return
        val tamanho = arquivoComando.length()
        val texto = try {
            if (tamanho > COMANDO_MAX_BYTES) null else arquivoComando.readText(Charsets.UTF_8)
        } catch (e: Exception) {
            Log.e(TAG, "FALHA sonda-ble COMANDO_LER ${e.javaClass.simpleName}")
            null
        }
        if (!arquivoComando.delete()) Log.e(TAG, "FALHA sonda-ble COMANDO_NAO_APAGOU")
        if (texto == null) { falha("COMANDO_INVALIDO", "motivo=ilegivel bytes=$tamanho"); return }
        // O app tem de estar na frente (decisão 3). O estado do ciclo de vida se lê na
        // thread principal.
        principal.post {
            val naFrente = (activity as? LifecycleOwner)?.lifecycle?.currentState?.isAtLeast(Lifecycle.State.RESUMED) ?: false
            h.post { executarComando(texto.trim(), naFrente) }
        }
    }

    private fun lerCampos(t: String): Map<String, String> =
        t.split(Regex("\\s+")).mapNotNull { p ->
            val i = p.indexOf('=')
            if (i <= 0) null else p.substring(0, i) to p.substring(i + 1)
        }.toMap()

    private fun fimComando(id: String, desfecho: String, codigo: String = "") =
        medida("comando-fim", "id=$id desfecho=$desfecho${if (codigo.isEmpty()) "" else " codigo=$codigo"}")

    private fun executarComando(t: String, naFrente: Boolean) {
        val c = lerCampos(t)
        val id = c["id"]?.takeIf { it.matches(Regex("[0-9]{1,6}")) }
        if (id == null) { falha("COMANDO_INVALIDO", "motivo=sem_id"); return }
        val acao = c["acao"]?.takeIf { it in ACOES }
        if (acao == null) { falha("COMANDO_INVALIDO", "id=$id motivo=acao"); fimComando(id, "recusado"); return }
        medida("comando", "id=$id acao=$acao")
        if (!naFrente) { falha("COMANDO_FORA_DA_FRENTE", "id=$id"); fimComando(id, "recusado", "COMANDO_FORA_DA_FRENTE"); return }
        when (acao) {
            "inventario" -> { inventario(); fimComando(id, "ok") }
            "parar" -> { parar("PEDIDO"); fimComando(id, "ok") }
            "anunciar" -> {
                val antes = servidor
                iniciarServidor("PAPEIS")
                if (servidor != null && antes == null) fimComando(id, "ok") else fimComando(id, "nao_comecou", ultimaFalha)
            }
            "l2cap-teste" -> { val ok = testarL2cap(); fimComando(id, if (ok) "ok" else "falha", if (ok) "" else ultimaFalha) }
            "roteiro" -> carregarRoteiro(id, c)
            else -> {
                val cfg = configDoComando(c, acao)
                if (cfg == null) { falha("COMANDO_INVALIDO", "id=$id motivo=configuracao"); fimComando(id, "recusado"); return }
                if (cliente != null || roteiro != null) { falha("CLIENTE_JA_ATIVO", "id=$id"); fimComando(id, "recusado", "CLIENTE_JA_ATIVO"); return }
                comandoCliente = id
                desfechoCliente = "parou"
                codigoDesfecho = ""
                if (cfg.modo == "AMBOS") iniciarServidor("AMBOS")
                iniciarCliente(cfg)
                // Recusado antes de começar (PROCURA_DEMAIS, Bluetooth desligado...): a
                // rodada não abriu e o pararCliente não vai marcar o fim.
                if (cliente == null && comandoCliente == id) {
                    comandoCliente = null
                    fimComando(id, "nao_comecou", ultimaFalha)
                }
            }
        }
    }

    private fun configDoComando(c: Map<String, String>, acao: String): Config? {
        val calibrar = acao == "calibrar"
        val cfg = Config(
            modo = if (acao == "ambos") "AMBOS" else "PAPEIS",
            phy = c["phy"] ?: "1M",
            prioridade = c["prioridade"] ?: "PADRAO",
            distancia = c["distancia"]?.toIntOrNull() ?: 1,
            repeticoes = c["repeticoes"]?.toIntOrNull() ?: if (calibrar) 3 else 5,
            calibrar = calibrar,
            volta = c["volta"] ?: "NOTIFICACAO",
            esperaAmbosMs = c["espera_ms"]?.toIntOrNull() ?: ESPERA_AMBOS_MAX_MS,
        )
        if (cfg.esperaAmbosMs !in 0..5_000) return null
        // A calibração depende de 3 rodadas: sabotagem na 1ª e na 2ª, nada na 3ª.
        if (calibrar && cfg.repeticoes < 3) return null
        return if (configValida(cfg)) cfg else null
    }

    /** Um aparelho só: a escuta L2CAP abre e dá um PSM da faixa dinâmica do BLE (0x80 a 0xFF). */
    private fun testarL2cap(): Boolean {
        if (!pronto()) return false
        return try {
            val e = adaptador!!.listenUsingInsecureL2capChannel()
            val psm = e.psm
            fecharQuieto(e)
            val valido = psm in 0x80..0xFF
            medida("l2cap-teste", "resultado=${if (valido) "ok" else "psm_fora"} psm=$psm")
            if (!valido) ultimaFalha = "L2CAP_PSM_FORA"
            valido
        } catch (e: IOException) {
            falha("L2CAP_ESCUTA", "erro=${e.javaClass.simpleName}")
            false
        }
    }

    // ---------- roteiro de alcance ----------

    private class Combo(val phy: String, val prioridade: String, val volta: String)

    private class Roteiro(val distancias: List<Int>, val combos: List<Combo>, val repeticoes: Int) {
        var ponto = 0
        var combo = 0
        var aguardandoToque = true
        var naoAchouNoPonto = 0
    }

    private var roteiro: Roteiro? = null
    // A rodada de cliente em andamento foi aberta pelo roteiro.
    private var rodadaDoRoteiro = false

    private fun carregarRoteiro(id: String, c: Map<String, String>) {
        if (cliente != null || roteiro != null) { falha("CLIENTE_JA_ATIVO", "id=$id"); fimComando(id, "recusado", "CLIENTE_JA_ATIVO"); return }
        val distancias = c["distancias"]?.split(',')?.map { it.toIntOrNull() ?: -1 } ?: emptyList()
        val combos = c["combos"]?.split(',')?.map { texto ->
            val p = texto.split('-')
            if (p.size == 3) Combo(p[0], p[1], p[2]) else Combo("", "", "")
        } ?: emptyList()
        val repeticoes = c["repeticoes"]?.toIntOrNull() ?: 5
        val ok = distancias.size in 1..10 && distancias.all { it in 1..500 } &&
            combos.size in 1..6 && repeticoes in 1..50 &&
            combos.all { configValida(Config("PAPEIS", it.phy, it.prioridade, 1, repeticoes, false, it.volta)) }
        if (!ok) { falha("COMANDO_INVALIDO", "id=$id motivo=roteiro"); fimComando(id, "recusado"); return }
        roteiro = Roteiro(distancias, combos, repeticoes)
        medida(
            "roteiro",
            "evento=carregado distancias=${distancias.joinToString(",")} " +
                "combos=${combos.joinToString(",") { "${it.phy}-${it.prioridade}-${it.volta}" }} repeticoes=$repeticoes",
        )
        fimComando(id, "ok")
        ocupado(true)
        avisarRoteiro()
    }

    private fun avisarRoteiro() {
        val r = roteiro
        val (texto, aguardando) = when {
            r == null -> "" to false
            r.aguardandoToque -> "Leve os dois celulares a ${r.distancias[r.ponto]} m um do outro e toque PRÓXIMO PONTO " +
                "(ponto ${r.ponto + 1} de ${r.distancias.size})" to true
            else -> "Medindo a ${r.distancias[r.ponto]} m: rodada ${r.combo + 1} de ${r.combos.size}. Não mexa nos celulares." to false
        }
        principal.post { canal.invokeMethod("roteiro", mapOf("texto" to texto, "aguardando" to aguardando)) }
    }

    /** O toque em PRÓXIMO PONTO: roda as combinações na distância da vez. */
    private fun proximoPonto() {
        val r = roteiro ?: return
        if (!r.aguardandoToque) return
        r.aguardandoToque = false
        r.combo = 0
        r.naoAchouNoPonto = 0
        medida("roteiro-ponto", "distancia=${r.distancias[r.ponto]} evento=comecou")
        rodarCombo()
    }

    private fun rodarCombo() {
        val r = roteiro ?: return
        if (r.aguardandoToque || cliente != null) return
        val k = r.combos[r.combo]
        avisarRoteiro()
        desfechoCliente = "parou"
        codigoDesfecho = ""
        rodadaDoRoteiro = true
        iniciarCliente(Config("PAPEIS", k.phy, k.prioridade, r.distancias[r.ponto], r.repeticoes, false, k.volta))
        if (cliente == null && rodadaDoRoteiro) {
            // Não abriu. Procura demais: espera a janela e tenta a mesma de novo.
            rodadaDoRoteiro = false
            if (ultimaFalha == "PROCURA_DEMAIS") { h.postDelayed({ rodarCombo() }, ESPERA_ENTRE_RODADAS_MS); return }
            desfechoCliente = "nao_comecou"
            codigoDesfecho = ultimaFalha
            depoisDaRodadaDoRoteiro()
        }
    }

    /** Chamado pelo pararCliente: a rodada do roteiro terminou (bem ou mal). */
    private fun roteiroAposRodada() {
        if (!rodadaDoRoteiro) return
        rodadaDoRoteiro = false
        h.post { depoisDaRodadaDoRoteiro() }
    }

    private fun depoisDaRodadaDoRoteiro() {
        val r = roteiro ?: return
        if (codigoDesfecho == "NAO_ACHOU_30S") r.naoAchouNoPonto++
        r.combo++
        if (r.combo < r.combos.size) {
            avisarRoteiro()
            h.postDelayed({ rodarCombo() }, ESPERA_ENTRE_RODADAS_MS)
            return
        }
        val d = r.distancias[r.ponto]
        medida("roteiro-ponto", "distancia=$d evento=fim rodadas=${r.combos.size} nao_achou=${r.naoAchouNoPonto}")
        // Nenhuma combinação achou o outro: daqui para longe também não vai achar.
        if (r.naoAchouNoPonto == r.combos.size) { fimRoteiro("SEM_ALCANCE_A_${d}M"); return }
        r.ponto++
        if (r.ponto == r.distancias.size) { fimRoteiro("COMPLETO"); return }
        r.aguardandoToque = true
        avisarRoteiro()
    }

    private fun fimRoteiro(motivo: String) {
        val r = roteiro ?: return
        roteiro = null
        rodadaDoRoteiro = false
        medida("roteiro-fim", "motivo=$motivo pontos_feitos=${r.ponto}/${r.distancias.size}")
        avisarRoteiro()
        if (cliente == null && servidor == null) ocupado(false)
    }

    // ---------- enviar as medidas como texto ----------

    /**
     * Abre o "Compartilhar" do Android com o arquivo de medidas inteiro, como texto (para
     * mandar pelo WhatsApp, por exemplo). Só códigos e números (regra 8.2). Abrir a escolha
     * tira o app da frente, então a sonda para o que estiver fazendo: a tela só deixa
     * compartilhar com a sonda parada.
     */
    private fun compartilhar(result: MethodChannel.Result) {
        val tamanho = arquivo.length()
        if (!arquivo.exists() || tamanho == 0L) {
            principal.post { result.error("SEM_MEDIDAS", "o arquivo de medidas esta vazio", null) }
            return
        }
        if (tamanho > COMPARTILHAR_MAX_BYTES) {
            principal.post { result.error("GRANDE_DEMAIS", "$tamanho bytes: use o -LerSonda no PC", null) }
            return
        }
        val texto = try {
            arquivo.readText(Charsets.UTF_8)
        } catch (e: Exception) {
            Log.e(TAG, "FALHA sonda-ble LER_ARQUIVO ${e.javaClass.simpleName}")
            principal.post { result.error("LER_ARQUIVO", e.javaClass.simpleName, null) }
            return
        }
        val linhas = texto.count { it == '\n' }
        val modelo = Build.MODEL.replace(Regex("[^A-Za-z0-9._-]"), "_")
        principal.post {
            try {
                val envio = Intent(Intent.ACTION_SEND).apply {
                    type = "text/plain"
                    putExtra(Intent.EXTRA_SUBJECT, "kryptos-sonda-$modelo")
                    putExtra(Intent.EXTRA_TEXT, "kryptos-sonda-$modelo ($linhas linhas)\n$texto")
                }
                activity.startActivity(Intent.createChooser(envio, "Enviar as medidas da sonda"))
                result.success(linhas)
            } catch (e: Exception) {
                Log.e(TAG, "FALHA sonda-ble COMPARTILHAR ${e.javaClass.simpleName}")
                result.error("COMPARTILHAR", e.javaClass.simpleName, null)
            }
        }
    }

    // ---------- permissões ----------

    fun permissoesConcedidas(): Boolean =
        PERMISSOES.all { contexto.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }

    private fun pedirPermissoes(result: MethodChannel.Result) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            h.post { falha("ANDROID_ANTIGO", "api=${Build.VERSION.SDK_INT}") }
            result.success(false)
            return
        }
        if (permissoesConcedidas()) {
            result.success(true)
            return
        }
        if (permissaoPendente != null) {
            result.error("PERMISSAO_EM_ANDAMENTO", "ja ha um pedido aberto", null)
            return
        }
        permissaoPendente = result
        // Um aviso só, "dispositivos por perto". Nenhuma permissão de localização.
        activity.requestPermissions(PERMISSOES, CODIGO_PERMISSAO)
    }

    /** Chamado pelo MainActivity (via CanaisDeTeste) com o resultado do aviso. */
    fun aoResultadoDePermissao(codigo: Int) {
        if (codigo != CODIGO_PERMISSAO) return
        val r = permissaoPendente ?: return
        permissaoPendente = null
        val ok = permissoesConcedidas()
        if (!ok) h.post { falha("SEM_PERMISSAO") }
        r.success(ok)
    }

    // ---------- inventário dos rádios ----------

    private fun sn(b: Boolean) = if (b) "sim" else "nao"

    private fun inventario(): String {
        val pm = contexto.packageManager
        val a = adaptador
        val partes = mutableListOf(
            "modelo=${Build.MODEL.replace(Regex("[^A-Za-z0-9._-]"), "_")}",
            "android=${Build.VERSION.RELEASE}",
            "api=${Build.VERSION.SDK_INT}",
        )
        if (a == null) {
            partes += "bluetooth=ausente"
        } else {
            try {
                partes += "bluetooth_ligado=${sn(a.isEnabled)}"
                partes += "le2m=${sn(a.isLe2MPhySupported)}"
                partes += "coded=${sn(a.isLeCodedPhySupported)}"
                partes += "anuncio_estendido=${sn(a.isLeExtendedAdvertisingSupported)}"
                partes += "anuncio_max_bytes=${a.leMaximumAdvertisingDataLength}"
                partes += "anuncios_multiplos=${sn(a.isMultipleAdvertisementSupported)}"
            } catch (e: SecurityException) {
                partes += "bluetooth=sem_permissao"
            }
        }
        partes += "wifi_aware=${sn(pm.hasSystemFeature(PackageManager.FEATURE_WIFI_AWARE))}"
        partes += "wifi_direct=${sn(pm.hasSystemFeature(PackageManager.FEATURE_WIFI_DIRECT))}"
        partes += "nfc=${sn(pm.hasSystemFeature(PackageManager.FEATURE_NFC))}"
        partes += "uwb=${sn(pm.hasSystemFeature("android.hardware.uwb"))}"
        val t = partes.joinToString(" ")
        medida("aparelho", t)
        // Tela acesa enquanto a sonda está aberta: apagar a tela tira o app da frente e a
        // sonda para (decisão 3 do modo-direto.md).
        principal.post { activity.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        return t
    }

    /** Confere o que toda operação precisa. Sem isso, registra a falha e devolve false. */
    private fun pronto(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            falha("ANDROID_ANTIGO", "api=${Build.VERSION.SDK_INT}"); return false
        }
        if (!permissoesConcedidas()) { falha("SEM_PERMISSAO"); return false }
        val a = adaptador
        if (a == null) { falha("SEM_BLUETOOTH"); return false }
        if (!a.isEnabled) { falha("BLUETOOTH_DESLIGADO"); return false }
        return true
    }

    // =====================================================================
    // SERVIDOR
    // =====================================================================

    private var servidor: BluetoothGattServer? = null
    private var servidorModo = ""
    private var servidorPronto = false
    private var saidaServidor: BluetoothGattCharacteristic? = null
    // O cliente aceito no OLA. Antes do OLA, uma conexão é só uma conexão: pode ser a que
    // este aparelho abriu como cliente (o Android avisa o servidor dela também).
    private var clienteDoServidor: BluetoothDevice? = null
    private val notificacaoLigadaPara = mutableSetOf<BluetoothDevice>()
    private val mtuPorAparelho = mutableMapOf<BluetoothDevice, Int>()
    private val semOlaAvisado = mutableSetOf<BluetoothDevice>()
    private var mtuServidor = 23
    private var remontagemServidor: QuadroSonda.Remontagem? = null
    private val conjuntos = mutableListOf<AdvertisingSetCallback>()

    // Envio por notificação (volta): fila e espera do aviso de envio.
    // Fila de notificações do servidor. O primeiro da fila está no ar enquanto
    // notificandoAgora; sai da fila no onNotificationSent. Quadro novo entra no fim: no
    // teste de 08/11/2026 o PEDIR_VOLTA chegou antes do aviso de envio do RESULTADO, e a
    // versão que recusava ("NOTIFICACAO_EM_ANDAMENTO") deixou a volta sem resposta.
    private val filaNotificacao = ArrayDeque<Pair<BluetoothDevice, ByteArray>>()
    private var notificandoAgora = false
    private var ocupadoNotificacao = 0
    private var voltaServidorN = 0
    private var voltaServidorComeco = 0L
    private var voltaServidorPedacos = 0
    private var voltaServidorCaminho = QuadroSonda.CAMINHO_NOTIFICACAO
    private val tempoVoltaServidor = Runnable {
        falha("TEMPO_ESGOTADO", "papel=servidor etapa=volta caminho=${QuadroSonda.nomeDoCaminho(voltaServidorCaminho)} n=$voltaServidorN")
        esvaziarNotificacoes()
        esvaziarReversa()
    }

    // Volta ESCRITA: este aparelho (servidor) conecta como cliente GATT no aparelho que
    // conectou nele, pelo mesmo enlace, e escreve sem resposta no serviço de volta de lá.
    private var reversa: BluetoothGatt? = null
    private var reversaEntrada: BluetoothGattCharacteristic? = null
    private var reversaPronta = false
    private var reversaMtu = 23
    private var reversaComeco = 0L
    private val filaReversa = ArrayDeque<ByteArray>()
    private var escrevendoReversa = false
    private var ocupadoReversa = 0
    private val tempoReversa = Runnable { if (reversa != null && !reversaPronta) prontoFalhou(QuadroSonda.CAMINHO_ESCRITA, "TEMPO_ESGOTADO") }
    private val tempoMtuReversa = Runnable { if (reversa != null && !reversaPronta && reversaEntrada != null) concluirReversa(mtuServidor, "herdado") }

    // Volta L2CAP: escuta num PSM dinâmico; o PSM vai ao cliente no PRONTO.
    private var l2capEscuta: BluetoothServerSocket? = null
    private var l2capServidor: BluetoothSocket? = null
    private var geracaoL2capServidor = 0
    private val escritorL2cap = Executors.newSingleThreadExecutor { Thread(it, "sonda-l2cap-escrita") }

    private fun iniciarServidor(modo: String) {
        if (!pronto()) return
        if (servidor != null) { falha("SERVIDOR_JA_ABERTO"); return }
        val g = gerente!!.openGattServer(contexto, retornoServidor)
        if (g == null) { falha("SERVIDOR_NAO_ABRIU"); return }
        servidor = g
        servidorModo = modo
        servidorPronto = false
        val servico = BluetoothGattService(UUID_SERVICO, BluetoothGattService.SERVICE_TYPE_PRIMARY)
        val entrada = BluetoothGattCharacteristic(
            UUID_ENTRADA,
            BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE or BluetoothGattCharacteristic.PROPERTY_WRITE,
            BluetoothGattCharacteristic.PERMISSION_WRITE,
        )
        val saida = BluetoothGattCharacteristic(UUID_SAIDA, BluetoothGattCharacteristic.PROPERTY_NOTIFY, 0)
        saida.addDescriptor(
            BluetoothGattDescriptor(UUID_CCCD, BluetoothGattDescriptor.PERMISSION_READ or BluetoothGattDescriptor.PERMISSION_WRITE),
        )
        servico.addCharacteristic(entrada)
        servico.addCharacteristic(saida)
        saidaServidor = saida
        ocupado(true)
        // Não anunciar antes do aviso de serviço adicionado (documentação do addService).
        if (!g.addService(servico)) { falha("SERVICO_RECUSADO"); fecharServidor(); return }
        h.postDelayed(tempoServico, TEMPO_OPERACAO_MS)
    }

    private val tempoServico = Runnable {
        if (servidor != null && !servidorPronto) { falha("TEMPO_ESGOTADO", "papel=servidor etapa=servico"); fecharServidor() }
    }

    private fun anunciar() {
        val anunciante = adaptador?.bluetoothLeAdvertiser
        if (anunciante == null) { falha("SEM_ANUNCIANTE"); return }
        val dados = AdvertiseData.Builder()
            .addServiceUuid(ParcelUuid(UUID_SERVICO))
            .setIncludeDeviceName(false)
            .setIncludeTxPowerLevel(false)
            .build()
        // Legado (1M): o que qualquer aparelho enxerga.
        val legado = AdvertisingSetParameters.Builder()
            .setLegacyMode(true).setConnectable(true).setScannable(true)
            .setInterval(AdvertisingSetParameters.INTERVAL_LOW)
            .setTxPowerLevel(AdvertisingSetParameters.TX_POWER_HIGH)
            .build()
        iniciarConjunto(anunciante, "legado", legado, dados, AdvertiseData.Builder().build())
        // Longo alcance (Coded): só no modo PAPEIS e se o aparelho tiver.
        val a: BluetoothAdapter = adaptador
        if (servidorModo == "PAPEIS") {
            if (a.isLeCodedPhySupported && a.isLeExtendedAdvertisingSupported) {
                val coded = AdvertisingSetParameters.Builder()
                    .setLegacyMode(false).setConnectable(true).setScannable(false)
                    .setPrimaryPhy(BluetoothDevice.PHY_LE_CODED)
                    .setSecondaryPhy(BluetoothDevice.PHY_LE_CODED)
                    .setInterval(AdvertisingSetParameters.INTERVAL_LOW)
                    .setTxPowerLevel(AdvertisingSetParameters.TX_POWER_HIGH)
                    .build()
                iniciarConjunto(anunciante, "coded", coded, dados, null)
            } else {
                medida("anuncio", "tipo=coded resultado=SEM_SUPORTE")
            }
        }
    }

    private fun iniciarConjunto(
        anunciante: android.bluetooth.le.BluetoothLeAdvertiser,
        tipo: String,
        p: AdvertisingSetParameters,
        dados: AdvertiseData,
        resposta: AdvertiseData?,
    ) {
        val cb = object : AdvertisingSetCallback() {
            override fun onAdvertisingSetStarted(set: AdvertisingSet?, txPower: Int, status: Int) {
                if (status == AdvertisingSetCallback.ADVERTISE_SUCCESS) medida("anuncio", "tipo=$tipo resultado=ok tx_dbm=$txPower")
                else falha("ANUNCIO_${tipo.uppercase(Locale.ROOT)}", "status=$status")
            }
        }
        try {
            anunciante.startAdvertisingSet(p, dados, resposta, null, null, cb, h)
            conjuntos += cb
        } catch (e: IllegalArgumentException) {
            falha("ANUNCIO_${tipo.uppercase(Locale.ROOT)}", "recusado=${e.javaClass.simpleName}")
        }
    }

    private val retornoServidor = object : BluetoothGattServerCallback() {
        override fun onServiceAdded(status: Int, service: BluetoothGattService?) {
            h.post {
                h.removeCallbacks(tempoServico)
                if (servidor == null) return@post
                if (status != BluetoothGatt.GATT_SUCCESS) { falha("SERVICO_RECUSADO", "status=$status"); fecharServidor(); return@post }
                servidorPronto = true
                medida("servidor", "modo=$servidorModo resultado=aberto")
                anunciar()
            }
        }

        override fun onConnectionStateChange(device: BluetoothDevice, status: Int, newState: Int) {
            h.post {
                if (newState == BluetoothProfile.STATE_CONNECTED) {
                    // Aceita a conexão; quem é o cliente se decide no OLA.
                    mtuPorAparelho[device] = 23
                    notificacaoLigadaPara -= device
                    medida("servidor", "evento=conectou status=$status")
                } else if (newState == BluetoothProfile.STATE_DISCONNECTED) {
                    mtuPorAparelho -= device
                    notificacaoLigadaPara -= device
                    semOlaAvisado -= device
                    descartarNotificacoes(device)
                    if (device == clienteDoServidor) {
                        clienteDoServidor = null
                        remontagemServidor = null
                        esvaziarNotificacoes()
                        fecharReversa()
                        fecharL2capServidor()
                        h.removeCallbacks(tempoVoltaServidor)
                        medida("servidor", "evento=desconectou status=$status")
                    }
                    notificarProximo() // o que era para outro aparelho continua
                }
            }
        }

        override fun onMtuChanged(device: BluetoothDevice, mtu: Int) {
            h.post {
                mtuPorAparelho[device] = mtu
                if (device == clienteDoServidor) mtuServidor = mtu
            }
        }

        override fun onDescriptorWriteRequest(
            device: BluetoothDevice, requestId: Int, descriptor: BluetoothGattDescriptor,
            preparedWrite: Boolean, responseNeeded: Boolean, offset: Int, value: ByteArray?,
        ) {
            val ok = descriptor.uuid == UUID_CCCD && value != null
            if (responseNeeded) {
                servidor?.sendResponse(device, requestId, if (ok) BluetoothGatt.GATT_SUCCESS else BluetoothGatt.GATT_FAILURE, offset, null)
            }
            val ligar = ok && value.contentEquals(BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
            h.post { if (ligar) notificacaoLigadaPara += device else notificacaoLigadaPara -= device }
        }

        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice, requestId: Int, characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean, responseNeeded: Boolean, offset: Int, value: ByteArray?,
        ) {
            if (responseNeeded) servidor?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, offset, null)
            val copia = value?.copyOf() ?: ByteArray(0)
            h.post { if (characteristic.uuid == UUID_ENTRADA) entradaDoServidor(device, copia) }
        }

        override fun onNotificationSent(device: BluetoothDevice, status: Int) {
            h.post {
                // Só conta o aviso do aparelho que está na frente da fila.
                if (!notificandoAgora || filaNotificacao.firstOrNull()?.first != device) return@post
                if (status != BluetoothGatt.GATT_SUCCESS) {
                    falha("NOTIFICACAO", "papel=servidor status=$status n=$voltaServidorN")
                    if (device == clienteDoServidor) esvaziarNotificacoes() else descartarNotificacoes(device)
                    notificarProximo()
                    return@post
                }
                filaNotificacao.removeFirst()
                notificandoAgora = false
                notificarProximo()
            }
        }
    }

    /** O OLA decide quem é o cliente deste servidor (ver decidirOla). */
    private fun tratarOla(device: BluetoothDevice, q: ByteArray) {
        val ola = QuadroSonda.lerOla(q)
        if (ola == null) { falha("QUADRO_INVALIDO", "papel=servidor tipo=ola"); return }
        if (device == clienteDoServidor) { responderOla(device, QuadroSonda.OLA_ACEITO); return } // repetido
        if (clienteDoServidor != null) {
            medida("servidor", "evento=ola_recusado motivo=ocupado")
            responderOla(device, QuadroSonda.OLA_RECUSADO_OCUPADO)
            return
        }
        val meuAtivo = cliente != null
        when (QuadroSonda.decidirOla(ola.nonce, meuNonce, meuAtivo)) {
            QuadroSonda.DECISAO_RECUSAR -> {
                // Os dois conectaram e o meu cliente vence: o outro vira só servidor.
                medida("ambos", "desfecho=colisao este=cliente")
                responderOla(device, QuadroSonda.OLA_RECUSADO_COLISAO)
                return
            }
            QuadroSonda.DECISAO_ACEITAR_E_PARAR_O_MEU -> {
                medida("ambos", if (meuNonce != null) "desfecho=colisao este=servidor" else "desfecho=servidor")
                desfechoCliente = "servidor"
                codigoDesfecho = ""
                pararCliente()
            }
        }
        clienteDoServidor = device
        mtuServidor = mtuPorAparelho[device] ?: 23
        remontagemServidor = null
        semOlaAvisado -= device
        medida("servidor", "evento=cliente_aceito modo=${if (ola.ambos) "AMBOS" else "PAPEIS"}")
        responderOla(device, QuadroSonda.OLA_ACEITO)
    }

    private fun responderOla(device: BluetoothDevice, codigo: Int) =
        notificar(listOf(QuadroSonda.olaResposta(codigo)), envioDeVolta = false, para = device)

    private fun entradaDoServidor(device: BluetoothDevice, q: ByteArray) {
        if (QuadroSonda.tipo(q) == QuadroSonda.TIPO_OLA) { tratarOla(device, q); return }
        if (device != clienteDoServidor) {
            // Quadro de quem não se apresentou (ou foi recusado): fora. Avisa uma vez.
            if (semOlaAvisado.add(device)) falha("QUADRO_SEM_OLA", "papel=servidor")
            return
        }
        when (QuadroSonda.tipo(q)) {
            QuadroSonda.TIPO_INICIO -> {
                val ini = QuadroSonda.lerInicio(q)
                if (ini == null) { falha("QUADRO_INVALIDO", "papel=servidor tipo=inicio"); return }
                remontagemServidor = QuadroSonda.Remontagem(ini) { SystemClock.elapsedRealtime() }
            }
            QuadroSonda.TIPO_PEDACO -> {
                val rem = remontagemServidor ?: return
                val r = rem.receber(q) ?: return
                medida(
                    "servidor-recebeu",
                    "n=${r.n} resultado=${QuadroSonda.nomeDoCodigo(r.codigo)} recebidos=${r.recebidos}/${rem.inicio.pedacos} " +
                        "repetidos=${rem.repetidos} ms=${r.msReceptor} mtu=$mtuServidor",
                )
                notificar(listOf(QuadroSonda.resultado(r)), envioDeVolta = false)
            }
            QuadroSonda.TIPO_PREPARAR -> {
                val c = QuadroSonda.lerPreparar(q)
                val d = clienteDoServidor
                if (c == null || d == null) { falha("QUADRO_INVALIDO", "papel=servidor tipo=preparar"); return }
                when (c) {
                    QuadroSonda.CAMINHO_ESCRITA -> abrirReversa(d)
                    QuadroSonda.CAMINHO_L2CAP -> abrirEscutaL2cap()
                }
            }
            QuadroSonda.TIPO_PEDIR_VOLTA -> {
                val p = QuadroSonda.lerPedirVolta(q)
                if (p == null) { falha("QUADRO_INVALIDO", "papel=servidor tipo=pedir_volta"); return }
                val nomeCaminho = QuadroSonda.nomeDoCaminho(p.caminho)
                val pronto = when (p.caminho) {
                    QuadroSonda.CAMINHO_ESCRITA -> reversaPronta
                    QuadroSonda.CAMINHO_L2CAP -> l2capServidor != null
                    else -> true
                }
                if (!pronto) { falha("VOLTA_NAO_PREPARADA", "papel=servidor caminho=$nomeCaminho n=${p.n}"); return }
                val dados = ByteArray(TAMANHO_CARTA).also { sorteio.nextBytes(it) }
                // L2CAP não tem o teto de 512 do atributo, mas usa o mesmo quadro para a
                // comparação ser só do caminho. O que vai no ar são pacotes de até 251 bytes
                // em qualquer caso (extensão de tamanho de dados do BLE).
                val mtu = when (p.caminho) {
                    QuadroSonda.CAMINHO_ESCRITA -> reversaMtu
                    QuadroSonda.CAMINHO_L2CAP -> MTU_PEDIDO
                    else -> mtuServidor
                }
                val tam = QuadroSonda.tamanhoDoPedaco(mtu)
                val quadros = listOf(QuadroSonda.inicio(p.n, dados, tam)) + QuadroSonda.pedacos(dados, tam, p.sabotagem)
                voltaServidorN = p.n
                voltaServidorCaminho = p.caminho
                voltaServidorPedacos = QuadroSonda.quantosPedacos(TAMANHO_CARTA, tam)
                voltaServidorComeco = SystemClock.elapsedRealtime()
                h.removeCallbacks(tempoVoltaServidor)
                h.postDelayed(tempoVoltaServidor, TEMPO_TRANSFERENCIA_MS)
                when (p.caminho) {
                    QuadroSonda.CAMINHO_ESCRITA -> escreverReversa(quadros)
                    QuadroSonda.CAMINHO_L2CAP -> enviarL2cap(quadros)
                    else -> notificar(quadros, envioDeVolta = true)
                }
            }
            QuadroSonda.TIPO_RESULTADO -> {
                val r = QuadroSonda.lerResultado(q)
                if (r == null) { falha("QUADRO_INVALIDO", "papel=servidor tipo=resultado"); return }
                h.removeCallbacks(tempoVoltaServidor)
                val ms = SystemClock.elapsedRealtime() - voltaServidorComeco
                val (ocupadoVolta, mtuVolta) = when (voltaServidorCaminho) {
                    QuadroSonda.CAMINHO_ESCRITA -> ocupadoReversa.toString() to reversaMtu.toString()
                    QuadroSonda.CAMINHO_L2CAP -> "-" to "-" // fluxo com créditos: não há "ocupado" nem MTU do GATT
                    else -> ocupadoNotificacao.toString() to mtuServidor.toString()
                }
                medida(
                    "servidor-enviou",
                    "n=${r.n} caminho=${QuadroSonda.nomeDoCaminho(voltaServidorCaminho)} bytes=$TAMANHO_CARTA pedacos=$voltaServidorPedacos " +
                        "ms=$ms kbps=${kbps(TAMANHO_CARTA, ms)} resultado=${QuadroSonda.nomeDoCodigo(r.codigo)} " +
                        "ocupado=$ocupadoVolta mtu=$mtuVolta",
                )
            }
            else -> falha("QUADRO_INVALIDO", "papel=servidor tipo=desconhecido")
        }
    }

    private fun notificar(quadros: List<ByteArray>, envioDeVolta: Boolean, para: BluetoothDevice? = clienteDoServidor) {
        val d = para ?: run { falha("SEM_CLIENTE", "papel=servidor"); return }
        if (d !in notificacaoLigadaPara) { falha("SEM_NOTIFICACAO", "papel=servidor"); return }
        if (envioDeVolta) ocupadoNotificacao = 0
        for (q in quadros) filaNotificacao.addLast(d to q)
        if (!notificandoAgora) notificarProximo()
    }

    /** Tira da fila o que era para um aparelho que saiu (ou foi recusado). */
    private fun descartarNotificacoes(device: BluetoothDevice) {
        val naFrente = filaNotificacao.firstOrNull()?.first == device
        filaNotificacao.removeAll { it.first == device }
        if (naFrente) notificandoAgora = false
    }

    private fun esvaziarNotificacoes() {
        filaNotificacao.clear()
        notificandoAgora = false
    }

    private fun notificarProximo() {
        val g = servidor ?: return
        val c = saidaServidor ?: return
        if (notificandoAgora || filaNotificacao.isEmpty()) return
        val (d, q) = filaNotificacao.first()
        val r = g.notifyCharacteristicChanged(d, c, false, q)
        if (r == BluetoothStatusCodes.SUCCESS) { notificandoAgora = true; return } // espera o onNotificationSent
        ocupadoNotificacao++
        if (ocupadoNotificacao > TENTATIVAS_OCUPADO_MAX) {
            falha("NOTIFICACAO_RECUSADA", "papel=servidor codigo=$r")
            esvaziarNotificacoes()
            return
        }
        h.postDelayed({ notificarProximo() }, 2)
    }

    private fun prontoFalhou(caminho: Int, motivo: String) {
        falha("VOLTA_PREPARAR", "papel=servidor caminho=${QuadroSonda.nomeDoCaminho(caminho)} motivo=$motivo")
        if (caminho == QuadroSonda.CAMINHO_ESCRITA) fecharReversa() else fecharL2capServidor()
        notificar(listOf(QuadroSonda.pronto(caminho, QuadroSonda.PRONTO_FALHOU)), envioDeVolta = false)
    }

    // ---------- volta ESCRITA (servidor como cliente GATT do cliente) ----------

    private fun abrirReversa(d: BluetoothDevice) {
        fecharReversa()
        reversaComeco = SystemClock.elapsedRealtime()
        // Mesmo aparelho: espero que o Android use o enlace que já existe (palpite; a
        // sonda mede pelo ms do servidor-volta e por VOLTA_PREPARAR).
        val g = d.connectGatt(contexto, false, retornoReversa, BluetoothDevice.TRANSPORT_LE, BluetoothDevice.PHY_LE_1M_MASK, h)
        if (g == null) { prontoFalhou(QuadroSonda.CAMINHO_ESCRITA, "CONEXAO_RECUSADA"); return }
        reversa = g
        h.postDelayed(tempoReversa, TEMPO_PREPARAR_MS)
    }

    private val retornoReversa = object : BluetoothGattCallback() {
        // Chega na thread da sonda (connectGatt com o Handler h).
        override fun onConnectionStateChange(g: BluetoothGatt, status: Int, newState: Int) {
            if (g != reversa) { g.close(); return }
            if (newState == BluetoothProfile.STATE_CONNECTED && status == BluetoothGatt.GATT_SUCCESS) {
                if (!reversaPronta && !g.discoverServices()) prontoFalhou(QuadroSonda.CAMINHO_ESCRITA, "SERVICOS_RECUSADO")
            } else {
                if (reversaPronta) falha("VOLTA_CAIU", "papel=servidor caminho=ESCRITA status=$status")
                else prontoFalhou(QuadroSonda.CAMINHO_ESCRITA, "CONEXAO_status_$status")
                fecharReversa()
            }
        }

        override fun onServicesDiscovered(g: BluetoothGatt, status: Int) {
            if (g != reversa || reversaPronta) return
            val c = g.getService(UUID_SERVICO_VOLTA)?.getCharacteristic(UUID_VOLTA_ENTRADA)
            if (status != BluetoothGatt.GATT_SUCCESS || c == null) {
                prontoFalhou(QuadroSonda.CAMINHO_ESCRITA, if (c == null) "SERVICO_AUSENTE" else "SERVICOS_status_$status")
                return
            }
            reversaEntrada = c
            // O MTU é do enlace e já foi trocado pelo outro lado; o pedido aqui pode não
            // ter resposta. Sem resposta em TEMPO_PHY_MS, usa o MTU que o servidor já sabe.
            if (!g.requestMtu(MTU_PEDIDO)) { concluirReversa(mtuServidor, "herdado"); return }
            h.postDelayed(tempoMtuReversa, TEMPO_PHY_MS)
        }

        override fun onMtuChanged(g: BluetoothGatt, mtu: Int, status: Int) {
            if (g != reversa || reversaPronta) return
            if (status == BluetoothGatt.GATT_SUCCESS) concluirReversa(mtu, "pedido") else concluirReversa(mtuServidor, "herdado")
        }

        override fun onCharacteristicWrite(g: BluetoothGatt, c: BluetoothGattCharacteristic, status: Int) {
            if (g != reversa) return
            escrevendoReversa = false
            if (status != BluetoothGatt.GATT_SUCCESS) {
                falha("ESCRITA", "papel=servidor caminho=ESCRITA status=$status n=$voltaServidorN")
                esvaziarReversa()
                return
            }
            if (filaReversa.isNotEmpty()) filaReversa.removeFirst()
            escreverReversaProximo()
        }
    }

    private fun concluirReversa(mtu: Int, origem: String) {
        h.removeCallbacks(tempoReversa)
        h.removeCallbacks(tempoMtuReversa)
        reversaPronta = true
        reversaMtu = mtu
        val ms = SystemClock.elapsedRealtime() - reversaComeco
        medida("servidor-volta", "caminho=ESCRITA resultado=pronta ms=$ms mtu=$mtu mtu_origem=$origem")
        notificar(listOf(QuadroSonda.pronto(QuadroSonda.CAMINHO_ESCRITA, QuadroSonda.PRONTO_OK)), envioDeVolta = false)
    }

    private fun escreverReversa(quadros: List<ByteArray>) {
        ocupadoReversa = 0
        filaReversa.addAll(quadros)
        escreverReversaProximo()
    }

    /** Mesmo controle de fluxo da ida: o próximo sai só depois do aviso da escrita. */
    private fun escreverReversaProximo() {
        val g = reversa ?: return
        val c = reversaEntrada ?: return
        if (escrevendoReversa || filaReversa.isEmpty()) return
        when (val r = g.writeCharacteristic(c, filaReversa.first(), BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE)) {
            BluetoothStatusCodes.SUCCESS -> escrevendoReversa = true
            BluetoothStatusCodes.ERROR_GATT_WRITE_REQUEST_BUSY -> {
                ocupadoReversa++
                if (ocupadoReversa > TENTATIVAS_OCUPADO_MAX) {
                    falha("ESCRITA_OCUPADA", "papel=servidor caminho=ESCRITA n=$voltaServidorN")
                    esvaziarReversa()
                } else {
                    h.postDelayed({ if (reversa === g) escreverReversaProximo() }, 2)
                }
            }
            else -> { falha("ESCRITA_RECUSADA", "papel=servidor caminho=ESCRITA codigo=$r"); esvaziarReversa() }
        }
    }

    private fun esvaziarReversa() {
        filaReversa.clear()
        escrevendoReversa = false
    }

    private fun fecharReversa() {
        h.removeCallbacks(tempoReversa)
        h.removeCallbacks(tempoMtuReversa)
        val g = reversa
        reversa = null
        reversaEntrada = null
        reversaPronta = false
        esvaziarReversa()
        try { g?.disconnect(); g?.close() } catch (e: Exception) { Log.e(TAG, "fechar volta: ${e.javaClass.simpleName}") }
    }

    // ---------- volta L2CAP ----------

    private fun abrirEscutaL2cap() {
        fecharL2capServidor()
        val escuta = try {
            adaptador!!.listenUsingInsecureL2capChannel()
        } catch (e: IOException) {
            prontoFalhou(QuadroSonda.CAMINHO_L2CAP, "ESCUTA_${e.javaClass.simpleName}")
            return
        }
        val geracao = ++geracaoL2capServidor
        l2capEscuta = escuta
        val comeco = SystemClock.elapsedRealtime()
        Thread({
            val s: BluetoothSocket? = try { escuta.accept(TEMPO_PREPARAR_MS.toInt()) } catch (e: IOException) { null }
            fecharQuieto(escuta) // um cliente só; o PSM fica livre
            h.post { aceitouL2cap(geracao, s, comeco) }
        }, "sonda-l2cap-escuta").start()
        notificar(listOf(QuadroSonda.pronto(QuadroSonda.CAMINHO_L2CAP, QuadroSonda.PRONTO_OK, escuta.psm)), envioDeVolta = false)
    }

    private fun aceitouL2cap(geracao: Int, s: BluetoothSocket?, comeco: Long) {
        if (geracao != geracaoL2capServidor || servidor == null) { fecharQuieto(s); return }
        l2capEscuta = null
        if (s == null) { falha("TEMPO_ESGOTADO", "papel=servidor etapa=l2cap_aceitar"); return }
        if (s.remoteDevice != clienteDoServidor) {
            fecharQuieto(s)
            medida("servidor", "evento=l2cap_outro_recusado")
            return
        }
        l2capServidor = s
        medida(
            "servidor-volta",
            "caminho=L2CAP resultado=pronta ms=${SystemClock.elapsedRealtime() - comeco} " +
                "tx_max=${s.maxTransmitPacketSize} rx_max=${s.maxReceivePacketSize}",
        )
    }

    /** Escreve numa thread própria: a escrita no canal bloqueia quando acabam os créditos. */
    private fun enviarL2cap(quadros: List<ByteArray>) {
        val s = l2capServidor ?: return
        val geracao = geracaoL2capServidor
        escritorL2cap.execute {
            try {
                val saida = BufferedOutputStream(s.outputStream, 8192)
                for (q in quadros) QuadroSonda.escreverMoldura(saida, q)
                saida.flush()
            } catch (e: IOException) {
                h.post {
                    if (geracao == geracaoL2capServidor) {
                        falha("L2CAP_ESCRITA", "papel=servidor erro=${e.javaClass.simpleName} n=$voltaServidorN")
                        fecharL2capServidor()
                    }
                }
            }
        }
    }

    private fun fecharL2capServidor() {
        geracaoL2capServidor++
        fecharQuieto(l2capEscuta)
        fecharQuieto(l2capServidor)
        l2capEscuta = null
        l2capServidor = null
    }

    private fun fecharQuieto(c: Closeable?) {
        try { c?.close() } catch (e: IOException) { Log.e(TAG, "fechar canal: ${e.javaClass.simpleName}") }
    }

    private fun fecharServidor() {
        h.removeCallbacks(tempoServico)
        h.removeCallbacks(tempoVoltaServidor)
        val anunciante = try { adaptador?.bluetoothLeAdvertiser } catch (e: SecurityException) { null }
        for (cb in conjuntos) {
            try { anunciante?.stopAdvertisingSet(cb) } catch (e: Exception) { Log.e(TAG, "parar anuncio: ${e.javaClass.simpleName}") }
        }
        conjuntos.clear()
        val g = servidor
        servidor = null
        try {
            clienteDoServidor?.let { g?.cancelConnection(it) }
            g?.clearServices()
            g?.close()
        } catch (e: Exception) {
            Log.e(TAG, "fechar servidor: ${e.javaClass.simpleName}")
        }
        clienteDoServidor = null
        notificacaoLigadaPara.clear()
        mtuPorAparelho.clear()
        semOlaAvisado.clear()
        remontagemServidor = null
        esvaziarNotificacoes()
        fecharReversa()
        fecharL2capServidor()
        servidorPronto = false
    }

    // =====================================================================
    // CLIENTE
    // =====================================================================

    private data class Config(
        val modo: String, val phy: String, val prioridade: String,
        val distancia: Int, val repeticoes: Int, val calibrar: Boolean,
        val volta: String,
        // Espera aleatória máxima antes de conectar no AMBOS; 0 provoca a colisão (teste do desempate).
        val esperaAmbosMs: Int = ESPERA_AMBOS_MAX_MS,
    ) {
        val caminho: Int get() = QuadroSonda.caminhoDoNome(volta) ?: QuadroSonda.CAMINHO_NOTIFICACAO
    }

    private sealed class Ev {
        data class Servicos(val status: Int) : Ev()
        data class Mtu(val mtu: Int, val status: Int) : Ev()
        data class Phy(val tx: Int, val rx: Int, val status: Int) : Ev()
        data class Descritor(val status: Int) : Ev()
        data class Escreveu(val status: Int) : Ev()
        data class Rssi(val rssi: Int, val status: Int) : Ev()
        data class ServicoVolta(val status: Int) : Ev()
    }

    private var cliente: Config? = null
    private var gatt: BluetoothGatt? = null
    // O aparelho achado na procura. Só comparado, nunca gravado (regra 8.2).
    private var dispositivoAlvo: BluetoothDevice? = null
    private var rodada = 0
    private var etapa = ""
    private var aoEvento: ((Ev) -> Boolean)? = null
    private var entradaCliente: BluetoothGattCharacteristic? = null
    private var mtuCliente = 23
    private var comecoConexao = 0L
    private var procurando = false
    private val procuras = ArrayDeque<Long>()
    private var idasOk = 0
    private var voltasOk = 0
    private val vistosIda = mutableListOf<String>()
    private val vistosVolta = mutableListOf<String>()

    private val tempoEsgotado = Runnable { falharCliente("TEMPO_ESGOTADO", "etapa=$etapa") }
    // Notificações do servidor chegam por aqui, à parte da espera das operações GATT.
    private var aoChegar: ((ByteArray) -> Unit)? = null
    private var etapaTransferencia = ""
    private val tempoTransferencia = Runnable { falharCliente("TEMPO_ESGOTADO", "etapa=$etapaTransferencia") }

    // Volta ESCRITA: serviço aberto por este aparelho (cliente) para o servidor escrever.
    private var servidorVolta: BluetoothGattServer? = null
    // Volta L2CAP: o canal aberto por este aparelho e o que está conectando.
    private var l2capCliente: BluetoothSocket? = null
    @Volatile private var l2capConectando: BluetoothSocket? = null
    private var geracaoL2capCliente = 0

    private fun campos(c: Config) =
        "rodada=$rodada modo=${c.modo} distancia=${c.distancia} phy=${c.phy} prioridade=${c.prioridade} " +
            "volta=${c.volta} calibrar=${sn(c.calibrar)}${if (c.modo == "AMBOS") " espera_ms=${c.esperaAmbosMs}" else ""}"

    /** Espera um evento. O tratador devolve true quando consumiu; false, continua esperando. */
    private fun esperar(nome: String, ms: Long, tratar: (Ev) -> Boolean) {
        etapa = nome
        aoEvento = tratar
        h.removeCallbacks(tempoEsgotado)
        h.postDelayed(tempoEsgotado, ms)
    }

    private fun entregar(ev: Ev) {
        val t = aoEvento ?: return
        if (t(ev) && aoEvento === t) {
            // consumido e ninguém pediu outra espera dentro do tratador
            aoEvento = null
            h.removeCallbacks(tempoEsgotado)
        }
    }

    /** Consome o evento e segue; usado dentro dos tratadores. */
    private fun seguir(acao: () -> Unit): Boolean {
        aoEvento = null
        h.removeCallbacks(tempoEsgotado)
        acao()
        return true
    }

    private fun iniciarCliente(cfg: Config) {
        if (!pronto()) return
        if (cliente != null) { falha("CLIENTE_JA_ATIVO"); return }
        val agora = SystemClock.elapsedRealtime()
        while (procuras.isNotEmpty() && agora - procuras.first() > JANELA_PROCURAS_MS) procuras.removeFirst()
        if (procuras.size >= PROCURAS_POR_JANELA) {
            val espere = (JANELA_PROCURAS_MS - (agora - procuras.first())) / 1000 + 1
            falha("PROCURA_DEMAIS", "espere_s=$espere")
            return
        }
        val scanner = adaptador!!.bluetoothLeScanner
        if (scanner == null) { falha("SEM_PROCURA"); return }
        rodada++
        cliente = cfg
        idasOk = 0; voltasOk = 0
        vistosIda.clear(); vistosVolta.clear()
        ocupado(true)
        val filtro = ScanFilter.Builder().setServiceUuid(ParcelUuid(UUID_SERVICO)).build()
        val ajustes = ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
        if (cfg.phy == "CODED") ajustes.setLegacy(false).setPhy(ScanSettings.PHY_LE_ALL_SUPPORTED)
        procuras.addLast(agora)
        comecoConexao = agora
        procurando = true
        etapa = "procura"
        try {
            scanner.startScan(listOf(filtro), ajustes.build(), retornoProcura)
        } catch (e: Exception) {
            procurando = false
            falharCliente("PROCURA_RECUSADA", "erro=${e.javaClass.simpleName}")
            return
        }
        h.removeCallbacks(tempoEsgotado)
        h.postDelayed(tempoProcura, TEMPO_PROCURA_MS)
    }

    private val tempoProcura = Runnable { if (procurando) falharCliente("NAO_ACHOU_30S") }

    private fun pararProcura() {
        h.removeCallbacks(tempoProcura)
        if (!procurando) return
        procurando = false
        try { adaptador?.bluetoothLeScanner?.stopScan(retornoProcura) } catch (e: Exception) { Log.e(TAG, "parar procura: ${e.javaClass.simpleName}") }
    }

    private val retornoProcura = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, result: ScanResult) {
            h.post { achou(result) }
        }

        override fun onScanFailed(errorCode: Int) {
            h.post { procurando = false; falharCliente("PROCURA_FALHOU", "codigo=$errorCode") }
        }
    }

    private fun nomePhy(p: Int) = when (p) {
        BluetoothDevice.PHY_LE_1M -> "1M"; BluetoothDevice.PHY_LE_2M -> "2M"; BluetoothDevice.PHY_LE_CODED -> "CODED"; else -> "P$p"
    }

    private fun achou(r: ScanResult) {
        val cfg = cliente ?: return
        if (!procurando) return
        // No modo CODED, só vale o anúncio de longo alcance.
        if (cfg.phy == "CODED" && r.primaryPhy != BluetoothDevice.PHY_LE_CODED) return
        if (!r.isConnectable) return
        pararProcura()
        val ms = SystemClock.elapsedRealtime() - comecoConexao
        val tx = if (r.txPower == ScanResult.TX_POWER_NOT_PRESENT) "?" else r.txPower.toString()
        medida("achou", "${campos(cfg)} phy_anuncio=${nomePhy(r.primaryPhy)} rssi=${r.rssi} tx_anuncio=$tx ms=$ms procuras_30s=${procuras.size}")
        val dispositivo = r.device
        if (cfg.modo == "AMBOS") {
            val espera = sorteio.nextInt(cfg.esperaAmbosMs + 1).toLong()
            etapa = "espera_ambos"
            h.postDelayed({ if (cliente === cfg && gatt == null) conectar(cfg, dispositivo) }, espera)
        } else {
            conectar(cfg, dispositivo)
        }
    }

    private fun conectar(cfg: Config, d: BluetoothDevice) {
        if (cfg.modo == "AMBOS" && clienteDoServidor != null) {
            medida("ambos", "desfecho=servidor")
            desfechoCliente = "servidor"
            pararCliente()
            return
        }
        comecoConexao = SystemClock.elapsedRealtime()
        dispositivoAlvo = d
        val mascara = if (cfg.phy == "CODED") BluetoothDevice.PHY_LE_CODED_MASK else BluetoothDevice.PHY_LE_1M_MASK
        etapa = "conexao"
        gatt = d.connectGatt(contexto, false, retornoCliente, BluetoothDevice.TRANSPORT_LE, mascara, h)
        if (gatt == null) { falharCliente("CONEXAO_RECUSADA"); return }
        h.removeCallbacks(tempoEsgotado)
        h.postDelayed(tempoEsgotado, TEMPO_CONEXAO_MS)
        if (cfg.modo == "AMBOS") medida("ambos", "desfecho=cliente")
    }

    private val retornoCliente = object : BluetoothGattCallback() {
        // Os retornos já chegam na thread da sonda (connectGatt com o Handler h).
        override fun onConnectionStateChange(g: BluetoothGatt, status: Int, newState: Int) {
            if (g != gatt) { g.close(); return }
            if (newState == BluetoothProfile.STATE_CONNECTED && status == BluetoothGatt.GATT_SUCCESS) {
                h.removeCallbacks(tempoEsgotado)
                aoConectar(g)
            } else if (newState == BluetoothProfile.STATE_DISCONNECTED || status != BluetoothGatt.GATT_SUCCESS) {
                if (cliente != null) falharCliente("CONEXAO_CAIU", "status=$status etapa=$etapa")
            }
        }

        override fun onServicesDiscovered(g: BluetoothGatt, status: Int) = entregar(Ev.Servicos(status))
        override fun onMtuChanged(g: BluetoothGatt, mtu: Int, status: Int) = entregar(Ev.Mtu(mtu, status))
        override fun onPhyUpdate(g: BluetoothGatt, txPhy: Int, rxPhy: Int, status: Int) = entregar(Ev.Phy(txPhy, rxPhy, status))
        override fun onPhyRead(g: BluetoothGatt, txPhy: Int, rxPhy: Int, status: Int) = entregar(Ev.Phy(txPhy, rxPhy, status))
        override fun onDescriptorWrite(g: BluetoothGatt, d: BluetoothGattDescriptor, status: Int) = entregar(Ev.Descritor(status))
        override fun onCharacteristicWrite(g: BluetoothGatt, c: BluetoothGattCharacteristic, status: Int) = entregar(Ev.Escreveu(status))
        override fun onCharacteristicChanged(g: BluetoothGatt, c: BluetoothGattCharacteristic, value: ByteArray) {
            if (c.uuid == UUID_SAIDA) aoChegar?.invoke(value.copyOf())
        }
        override fun onReadRemoteRssi(g: BluetoothGatt, rssi: Int, status: Int) = entregar(Ev.Rssi(rssi, status))
    }

    private fun aoConectar(g: BluetoothGatt) {
        val cfg = cliente ?: return
        val msConectar = SystemClock.elapsedRealtime() - comecoConexao
        if (cfg.prioridade == "ALTA") {
            if (!g.requestConnectionPriority(BluetoothGatt.CONNECTION_PRIORITY_HIGH)) falha("PRIORIDADE_RECUSADA", campos(cfg))
        }
        if (!g.discoverServices()) { falharCliente("SERVICOS_RECUSADO"); return }
        esperar("servicos", TEMPO_OPERACAO_MS) { ev ->
            if (ev !is Ev.Servicos) return@esperar false
            seguir {
                if (ev.status != BluetoothGatt.GATT_SUCCESS) { falharCliente("SERVICOS", "status=${ev.status}"); return@seguir }
                val s = g.getService(UUID_SERVICO)
                val entrada = s?.getCharacteristic(UUID_ENTRADA)
                val saida = s?.getCharacteristic(UUID_SAIDA)
                if (entrada == null || saida == null) { falharCliente("SERVICO_AUSENTE"); return@seguir }
                entradaCliente = entrada
                pedirMtu(g, cfg, saida, msConectar)
            }
        }
    }

    private fun pedirMtu(g: BluetoothGatt, cfg: Config, saida: BluetoothGattCharacteristic, msConectar: Long) {
        // Android 14+: o primeiro pedido vira 517 e os seguintes são ignorados. Um pedido só.
        if (!g.requestMtu(MTU_PEDIDO)) { falharCliente("MTU_RECUSADO"); return }
        esperar("mtu", TEMPO_OPERACAO_MS) { ev ->
            if (ev !is Ev.Mtu) return@esperar false
            seguir {
                mtuCliente = if (ev.status == BluetoothGatt.GATT_SUCCESS) ev.mtu else 23
                if (ev.status != BluetoothGatt.GATT_SUCCESS) falha("MTU", "status=${ev.status} ${campos(cfg)}")
                ajustarPhy(g, cfg, saida, msConectar)
            }
        }
    }

    private fun ajustarPhy(g: BluetoothGatt, cfg: Config, saida: BluetoothGattCharacteristic, msConectar: Long) {
        val pedido = when (cfg.phy) {
            "2M" -> Triple(BluetoothDevice.PHY_LE_2M_MASK, BluetoothDevice.PHY_LE_2M_MASK, BluetoothDevice.PHY_OPTION_NO_PREFERRED)
            "CODED" -> Triple(BluetoothDevice.PHY_LE_CODED_MASK, BluetoothDevice.PHY_LE_CODED_MASK, BluetoothDevice.PHY_OPTION_S8)
            else -> null
        }
        if (pedido == null) { lerPhy(g, cfg, saida, msConectar); return }
        g.setPreferredPhy(pedido.first, pedido.second, pedido.third)
        // Alguns aparelhos não avisam quando o PHY já é o pedido: o tempo esgotado aqui não é falha.
        etapa = "phy"
        aoEvento = { ev -> if (ev is Ev.Phy) seguir { lerPhy(g, cfg, saida, msConectar) } else false }
        h.removeCallbacks(tempoEsgotado)
        h.postDelayed({
            if (etapa == "phy" && aoEvento != null) { aoEvento = null; lerPhy(g, cfg, saida, msConectar) }
        }, TEMPO_PHY_MS)
    }

    private fun lerPhy(g: BluetoothGatt, cfg: Config, saida: BluetoothGattCharacteristic, msConectar: Long) {
        g.readPhy()
        esperar("ler_phy", TEMPO_OPERACAO_MS) { ev ->
            if (ev !is Ev.Phy) return@esperar false
            seguir {
                val pedaco = QuadroSonda.tamanhoDoPedaco(mtuCliente)
                medida(
                    "conexao",
                    "${campos(cfg)} ms=$msConectar mtu=$mtuCliente pedaco=$pedaco " +
                        "pedacos=${QuadroSonda.quantosPedacos(TAMANHO_CARTA, pedaco)} phy_tx=${nomePhy(ev.tx)} phy_rx=${nomePhy(ev.rx)}",
                )
                ligarNotificacao(g, cfg, saida)
            }
        }
    }

    private fun ligarNotificacao(g: BluetoothGatt, cfg: Config, saida: BluetoothGattCharacteristic) {
        val d = saida.getDescriptor(UUID_CCCD)
        if (d == null || !g.setCharacteristicNotification(saida, true)) { falharCliente("NOTIFICACAO_RECUSADA"); return }
        val r = g.writeDescriptor(d, BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
        if (r != BluetoothStatusCodes.SUCCESS) { falharCliente("NOTIFICACAO_RECUSADA", "codigo=$r"); return }
        esperar("notificacao", TEMPO_OPERACAO_MS) { ev ->
            if (ev !is Ev.Descritor) return@esperar false
            seguir {
                if (ev.status != BluetoothGatt.GATT_SUCCESS) { falharCliente("NOTIFICACAO", "status=${ev.status}"); return@seguir }
                apresentar(g, cfg)
            }
        }
    }

    // O nonce do OLA deste cliente; null enquanto não mandou (ver decidirOla).
    private var meuNonce: Long? = null

    /** OLA: o primeiro quadro, com um nonce novo. Só segue com a resposta ACEITO. */
    private fun apresentar(g: BluetoothGatt, cfg: Config) {
        val nonce = sorteio.nextLong()
        meuNonce = nonce
        val comeco = SystemClock.elapsedRealtime()
        var resposta: Int? = null
        var escritaFim = false
        fun talvezSeguir() {
            val r = resposta ?: return
            if (!escritaFim) return
            pararTransferencia()
            when (r) {
                QuadroSonda.OLA_ACEITO -> {
                    medida("ola", "${campos(cfg)} resultado=aceito ms=${SystemClock.elapsedRealtime() - comeco}")
                    when (cfg.caminho) {
                        QuadroSonda.CAMINHO_ESCRITA -> abrirServidorVolta(g, cfg)
                        QuadroSonda.CAMINHO_L2CAP -> prepararVolta(g, cfg, SystemClock.elapsedRealtime())
                        else -> ida(g, cfg, 1)
                    }
                }
                QuadroSonda.OLA_RECUSADO_COLISAO -> {
                    // O outro venceu o desempate: este fica só de servidor para ele.
                    medida("ambos", "desfecho=colisao este=servidor lado=cliente")
                    desfechoCliente = "servidor"
                    codigoDesfecho = ""
                    pararCliente()
                }
                else -> falharCliente("OLA_RECUSADO", "codigo=$r")
            }
        }
        iniciarTransferencia("ola") { v ->
            val r = QuadroSonda.lerOlaResposta(v)
            if (r != null && resposta == null) { resposta = r; talvezSeguir() }
        }
        escrever(g, listOf(QuadroSonda.ola(nonce, cfg.modo == "AMBOS")), 0, IntArray(1)) { escritaFim = true; talvezSeguir() }
    }

    private fun sabotagem(cfg: Config, n: Int): Int = when {
        !cfg.calibrar -> QuadroSonda.SABOTAGEM_NENHUMA
        n == 1 -> QuadroSonda.SABOTAGEM_TROCAR_BYTE
        n == 2 -> QuadroSonda.SABOTAGEM_PULAR_PEDACO
        else -> QuadroSonda.SABOTAGEM_NENHUMA
    }

    // ---------- preparar a volta ESCRITA ou L2CAP ----------

    private fun abrirServidorVolta(g: BluetoothGatt, cfg: Config) {
        val comeco = SystemClock.elapsedRealtime()
        val sv = gerente!!.openGattServer(contexto, retornoServidorVolta)
        if (sv == null) { falharCliente("SERVIDOR_VOLTA_NAO_ABRIU"); return }
        servidorVolta = sv
        val servico = BluetoothGattService(UUID_SERVICO_VOLTA, BluetoothGattService.SERVICE_TYPE_PRIMARY)
        servico.addCharacteristic(
            BluetoothGattCharacteristic(
                UUID_VOLTA_ENTRADA,
                BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE or BluetoothGattCharacteristic.PROPERTY_WRITE,
                BluetoothGattCharacteristic.PERMISSION_WRITE,
            ),
        )
        if (!sv.addService(servico)) { falharCliente("SERVICO_VOLTA_RECUSADO"); return }
        esperar("servico_volta", TEMPO_OPERACAO_MS) { ev ->
            if (ev !is Ev.ServicoVolta) return@esperar false
            seguir {
                if (ev.status != BluetoothGatt.GATT_SUCCESS) falharCliente("SERVICO_VOLTA_RECUSADO", "status=${ev.status}")
                else prepararVolta(g, cfg, comeco)
            }
        }
    }

    private val retornoServidorVolta = object : BluetoothGattServerCallback() {
        override fun onServiceAdded(status: Int, service: BluetoothGattService?) {
            h.post { entregar(Ev.ServicoVolta(status)) }
        }

        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice, requestId: Int, characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean, responseNeeded: Boolean, offset: Int, value: ByteArray?,
        ) {
            if (responseNeeded) servidorVolta?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, offset, null)
            val copia = value?.copyOf() ?: ByteArray(0)
            // Só aceita do aparelho em que este conectou.
            h.post { if (device == dispositivoAlvo && characteristic.uuid == UUID_VOLTA_ENTRADA) aoChegar?.invoke(copia) }
        }
    }

    /** Manda PREPARAR e espera o PRONTO (e o aviso da escrita do PREPARAR). */
    private fun prepararVolta(g: BluetoothGatt, cfg: Config, comeco: Long) {
        var pronto: QuadroSonda.Pronto? = null
        var escritaFim = false
        fun talvezSeguir() {
            val p = pronto ?: return
            if (!escritaFim) return
            pararTransferencia()
            if (p.codigo != QuadroSonda.PRONTO_OK) {
                falharCliente("VOLTA_PREPARAR", "codigo=${p.codigo}")
                return
            }
            if (cfg.caminho == QuadroSonda.CAMINHO_L2CAP) {
                conectarL2cap(g, cfg, p.psm, comeco)
            } else {
                medida("volta-pronta", "${campos(cfg)} ms=${SystemClock.elapsedRealtime() - comeco}")
                ida(g, cfg, 1)
            }
        }
        iniciarTransferencia("preparar") { v ->
            val p = QuadroSonda.lerPronto(v)
            if (p != null && p.caminho == cfg.caminho && pronto == null) { pronto = p; talvezSeguir() }
        }
        escrever(g, listOf(QuadroSonda.preparar(cfg.caminho)), 0, IntArray(1)) { escritaFim = true; talvezSeguir() }
    }

    private fun conectarL2cap(g: BluetoothGatt, cfg: Config, psm: Int, comeco: Long) {
        val d = dispositivoAlvo ?: return
        val geracao = ++geracaoL2capCliente
        etapa = "l2cap"
        h.removeCallbacks(tempoEsgotado)
        h.postDelayed(tempoEsgotado, TEMPO_CONEXAO_MS)
        Thread({
            var s: BluetoothSocket? = null
            val erro = try {
                s = d.createInsecureL2capChannel(psm)
                l2capConectando = s
                s.connect() // bloqueia; fechar o socket de outra thread desbloqueia
                null
            } catch (e: IOException) {
                fecharQuieto(s)
                e.javaClass.simpleName
            } finally {
                l2capConectando = null
            }
            h.post {
                if (geracao != geracaoL2capCliente || cliente !== cfg) { fecharQuieto(s); return@post }
                h.removeCallbacks(tempoEsgotado)
                val canal = s
                if (erro != null || canal == null) { falharCliente("L2CAP_CONEXAO", "erro=$erro"); return@post }
                l2capCliente = canal
                lerL2cap(canal, geracao)
                medida(
                    "volta-pronta",
                    "${campos(cfg)} ms=${SystemClock.elapsedRealtime() - comeco} " +
                        "tx_max=${canal.maxTransmitPacketSize} rx_max=${canal.maxReceivePacketSize}",
                )
                ida(g, cfg, 1)
            }
        }, "sonda-l2cap-conectar").start()
    }

    /** Lê molduras numa thread própria e entrega cada quadro na thread da sonda. */
    private fun lerL2cap(s: BluetoothSocket, geracao: Int) {
        Thread({
            val erro = try {
                val entrada = BufferedInputStream(s.inputStream, 8192)
                while (true) {
                    val q = QuadroSonda.lerMoldura(entrada) ?: break
                    h.post { if (geracao == geracaoL2capCliente) aoChegar?.invoke(q) }
                }
                "FECHADO"
            } catch (e: IOException) {
                e.javaClass.simpleName
            }
            h.post { if (geracao == geracaoL2capCliente && cliente != null) falharCliente("L2CAP_CAIU", "erro=$erro etapa=$etapa") }
        }, "sonda-l2cap-leitura").start()
    }

    private fun fecharVoltaCliente() {
        geracaoL2capCliente++
        fecharQuieto(l2capConectando)
        fecharQuieto(l2capCliente)
        l2capCliente = null
        val sv = servidorVolta
        servidorVolta = null
        try { sv?.clearServices(); sv?.close() } catch (e: Exception) { Log.e(TAG, "fechar servico de volta: ${e.javaClass.simpleName}") }
    }

    // ---------- ida: o cliente escreve sem resposta, o servidor confere ----------

    private fun ida(g: BluetoothGatt, cfg: Config, n: Int) {
        val dados = ByteArray(TAMANHO_CARTA).also { sorteio.nextBytes(it) }
        val tam = QuadroSonda.tamanhoDoPedaco(mtuCliente)
        val quadros = listOf(QuadroSonda.inicio(n, dados, tam)) + QuadroSonda.pedacos(dados, tam, sabotagem(cfg, n))
        val comeco = SystemClock.elapsedRealtime()
        val ocupadoIda = IntArray(1)
        var resultado: QuadroSonda.Resultado? = null
        var msResultado = 0L
        var escritaFim = false
        // O RESULTADO pode chegar antes do aviso da última escrita: segue quando os dois chegaram.
        fun talvezSeguir() {
            val r = resultado ?: return
            if (!escritaFim) return
            pararTransferencia()
            val nome = QuadroSonda.nomeDoCodigo(r.codigo)
            vistosIda += nome
            if (r.codigo == QuadroSonda.CONFERIU) idasOk++
            lerRssi(g) { rssi ->
                medida(
                    "ida",
                    "${campos(cfg)} n=$n bytes=$TAMANHO_CARTA pedacos=${QuadroSonda.quantosPedacos(TAMANHO_CARTA, tam)} " +
                        "ms=$msResultado kbps=${kbps(TAMANHO_CARTA, msResultado)} resultado=$nome recebidos=${r.recebidos} " +
                        "ms_receptor=${r.msReceptor} ocupado=${ocupadoIda[0]} rssi=$rssi",
                )
                volta(g, cfg, n)
            }
        }
        iniciarTransferencia("ida n=$n") { v ->
            val r = QuadroSonda.lerResultado(v)
            if (r != null && resultado == null) {
                resultado = r
                msResultado = SystemClock.elapsedRealtime() - comeco
                talvezSeguir()
            }
        }
        escrever(g, quadros, 0, ocupadoIda) { escritaFim = true; talvezSeguir() }
    }

    private fun iniciarTransferencia(nome: String, aoChegarNotificacao: (ByteArray) -> Unit) {
        etapaTransferencia = nome
        aoChegar = aoChegarNotificacao
        h.removeCallbacks(tempoTransferencia)
        h.postDelayed(tempoTransferencia, TEMPO_TRANSFERENCIA_MS)
    }

    private fun pararTransferencia() {
        aoChegar = null
        h.removeCallbacks(tempoTransferencia)
    }

    private fun escrever(g: BluetoothGatt, quadros: List<ByteArray>, i: Int, ocupadoConta: IntArray, aoFim: () -> Unit) {
        if (cliente == null) return
        if (i == quadros.size) { aoFim(); return }
        val c = entradaCliente ?: return
        val r = g.writeCharacteristic(c, quadros[i], BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE)
        when (r) {
            // Sem resposta: o Android só avisa a escrita quando a fila interna tem espaço.
            // É o controle de fluxo; o próximo pedaço sai só depois do aviso.
            BluetoothStatusCodes.SUCCESS -> esperar("escrita", TEMPO_OPERACAO_MS) { ev ->
                if (ev !is Ev.Escreveu) return@esperar false
                seguir {
                    if (ev.status != BluetoothGatt.GATT_SUCCESS) falharCliente("ESCRITA", "status=${ev.status} etapa=$etapa")
                    else escrever(g, quadros, i + 1, ocupadoConta, aoFim)
                }
            }
            BluetoothStatusCodes.ERROR_GATT_WRITE_REQUEST_BUSY -> {
                ocupadoConta[0]++
                if (ocupadoConta[0] > TENTATIVAS_OCUPADO_MAX) { falharCliente("ESCRITA_OCUPADA"); return }
                h.postDelayed({ if (gatt === g) escrever(g, quadros, i, ocupadoConta, aoFim) }, 2)
            }
            else -> falharCliente("ESCRITA_RECUSADA", "codigo=$r")
        }
    }

    private fun lerRssi(g: BluetoothGatt, depois: (String) -> Unit) {
        if (!g.readRemoteRssi()) { depois("?"); return }
        esperar("rssi", TEMPO_OPERACAO_MS) { ev ->
            if (ev !is Ev.Rssi) return@esperar false
            seguir { depois(if (ev.status == BluetoothGatt.GATT_SUCCESS) ev.rssi.toString() else "?") }
        }
    }

    // ---------- volta: o servidor manda pelo caminho escolhido, o cliente confere ----------

    private fun volta(g: BluetoothGatt, cfg: Config, n: Int) {
        val comeco = SystemClock.elapsedRealtime()
        var rem: QuadroSonda.Remontagem? = null
        var resultado: QuadroSonda.Resultado? = null
        var msResultado = 0L
        var escritaFim = false
        // O INICIO e os pedaços podem chegar antes do aviso da escrita do pedido.
        fun talvezSeguir() {
            val r = resultado ?: return
            val atual = rem ?: return
            if (!escritaFim) return
            pararTransferencia()
            val nome = QuadroSonda.nomeDoCodigo(r.codigo)
            vistosVolta += nome
            if (r.codigo == QuadroSonda.CONFERIU) voltasOk++
            medida(
                "volta",
                "${campos(cfg)} n=$n bytes=$TAMANHO_CARTA pedacos=${atual.inicio.pedacos} ms=$msResultado " +
                    "kbps=${kbps(TAMANHO_CARTA, msResultado)} resultado=$nome recebidos=${r.recebidos} " +
                    "repetidos=${atual.repetidos} ms_receptor=${r.msReceptor}",
            )
            // Devolve o resultado para o servidor medir o lado dele.
            escrever(g, listOf(QuadroSonda.resultado(r)), 0, IntArray(1)) {
                if (n < cfg.repeticoes) ida(g, cfg, n + 1) else terminarRodada(cfg)
            }
        }
        iniciarTransferencia("volta n=$n") { v ->
            when (QuadroSonda.tipo(v)) {
                QuadroSonda.TIPO_INICIO -> {
                    val ini = QuadroSonda.lerInicio(v)
                    if (ini == null) falharCliente("QUADRO_INVALIDO", "etapa=volta tipo=inicio")
                    else if (rem == null) rem = QuadroSonda.Remontagem(ini) { SystemClock.elapsedRealtime() }
                }
                QuadroSonda.TIPO_PEDACO -> {
                    val r = rem?.receber(v)
                    if (r != null && resultado == null) {
                        resultado = r
                        msResultado = SystemClock.elapsedRealtime() - comeco
                        talvezSeguir()
                    }
                }
                else -> Unit
            }
        }
        escrever(g, listOf(QuadroSonda.pedirVolta(n, sabotagem(cfg, n), cfg.caminho)), 0, IntArray(1)) { escritaFim = true; talvezSeguir() }
    }

    private fun terminarRodada(cfg: Config) {
        desfechoCliente = "ok"
        codigoDesfecho = ""
        medida("rodada-fim", "${campos(cfg)} idas_ok=$idasOk/${cfg.repeticoes} voltas_ok=$voltasOk/${cfg.repeticoes}")
        if (cfg.calibrar) {
            val esperado = listOf("SHA_NAO_CONFERE", "PEDACO_FALTANDO") + List(cfg.repeticoes - 2) { "CONFERIU" }
            val ok = vistosIda == esperado && vistosVolta == esperado
            desfechoCliente = if (ok) "calibracao_OK" else "calibracao_FALHOU"
            medida(
                "calibracao",
                "rodada=$rodada volta=${cfg.volta} resultado=${if (ok) "OK" else "FALHOU"} ida=${vistosIda.joinToString(",")} voltas=${vistosVolta.joinToString(",")}",
            )
        }
        pararCliente()
    }

    private fun falharCliente(codigo: String, extra: String = "") {
        val cfg = cliente
        if (cfg == null) { falha(codigo, extra); return }
        desfechoCliente = "falha"
        codigoDesfecho = codigo
        falha(codigo, "${campos(cfg)}${if (extra.isEmpty()) "" else " $extra"}")
        pararCliente()
    }

    private fun pararCliente() {
        pararProcura()
        h.removeCallbacks(tempoEsgotado)
        pararTransferencia()
        aoEvento = null
        val g = gatt
        gatt = null
        cliente = null
        dispositivoAlvo = null
        entradaCliente = null
        meuNonce = null
        fecharVoltaCliente()
        try {
            g?.disconnect()
            g?.close()
        } catch (e: Exception) {
            Log.e(TAG, "fechar cliente: ${e.javaClass.simpleName}")
        }
        val id = comandoCliente
        if (id != null) {
            comandoCliente = null
            fimComando(id, desfechoCliente, codigoDesfecho)
        }
        roteiroAposRodada()
        if (servidor == null && roteiro == null) ocupado(false)
    }

    // =====================================================================

    /** Para tudo: procura, anúncio, conexão e servidor. Também quando o app sai da frente. */
    fun parar(motivo: String) {
        // O roteiro sai antes do cliente, para a rodada interrompida não puxar a próxima.
        if (roteiro != null) fimRoteiro(motivo)
        val haviaAlgo = cliente != null || servidor != null
        if (cliente != null) falharCliente(if (motivo == "PEDIDO") "PARADA_PEDIDA" else motivo)
        if (servidor != null) {
            if (motivo != "PEDIDO") falha(motivo, "papel=servidor")
            fecharServidor()
        }
        if (haviaAlgo) medida("parou", "motivo=$motivo")
        ocupado(false)
    }

    /** Chamado do lado do Android (pausa e destruição da Activity). */
    fun pararDeFora(motivo: String) {
        h.post { parar(motivo) }
    }

    fun encerrar() {
        h.post {
            parar("APP_FECHOU")
            escritorL2cap.shutdownNow()
            linha.quitSafely()
        }
    }

    private fun kbps(bytes: Int, ms: Long): String =
        if (ms <= 0) "?" else String.format(Locale.ROOT, "%.1f", bytes * 8.0 / ms)
}
