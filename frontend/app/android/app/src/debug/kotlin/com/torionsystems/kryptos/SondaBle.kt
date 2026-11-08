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
import android.bluetooth.BluetoothStatusCodes
import android.bluetooth.le.AdvertiseData
import android.bluetooth.le.AdvertisingSet
import android.bluetooth.le.AdvertisingSetCallback
import android.bluetooth.le.AdvertisingSetParameters
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.ParcelUuid
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.io.FileOutputStream
import java.security.SecureRandom
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.UUID

/**
 * Sonda medida do modo Direto (documentacao/arquitetura/modo-direto.md, passo 1). Só no app
 * de debug. Sem protocolo do Kryptos: manda bytes aleatórios do tamanho da maior carta e
 * mede.
 *
 * Papéis:
 *  - SERVIDOR: abre o serviço GATT e anuncia em dois conjuntos, o legado (1M) e, se o
 *    aparelho tiver, o de longo alcance (PHY Coded, anúncio estendido).
 *  - CLIENTE: procura, conecta, pede MTU, ajusta PHY e prioridade, e faz n idas (cliente
 *    escreve sem resposta) e n voltas (servidor notifica), cada uma conferida pelo SHA-256.
 *  - AMBOS: os dois papéis ao mesmo tempo; o desempate é uma espera aleatória antes de
 *    conectar, sem nenhum dado a mais no anúncio.
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

        // A maior carta medida (transportes.md, texto de 64 KiB). Medida, não estimada.
        const val TAMANHO_CARTA = 66_594
        const val MTU_PEDIDO = 517

        const val TEMPO_PROCURA_MS = 30_000L
        const val TEMPO_CONEXAO_MS = 15_000L
        const val TEMPO_OPERACAO_MS = 10_000L
        const val TEMPO_PHY_MS = 3_000L
        const val TEMPO_TRANSFERENCIA_MS = 30_000L
        // Limite do Android: 5 startScan em 30 s por app; a sexta fica muda, sem erro.
        const val PROCURAS_POR_JANELA = 5
        const val JANELA_PROCURAS_MS = 30_000L
        const val ESPERA_AMBOS_MAX_MS = 1_500
        const val TENTATIVAS_OCUPADO_MAX = 5_000
        const val ARQUIVO = "sonda-ble.txt"
        const val ARQUIVO_MAX_BYTES = 2L * 1024 * 1024

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

    private fun falha(codigo: String, campos: String = "") =
        registrar("FALHA sonda-ble $codigo${if (campos.isEmpty()) "" else " $campos"}")

    private fun ocupado(sim: Boolean) = principal.post { canal.invokeMethod("ocupado", sim) }

    // ---------- canal com o Dart ----------

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "permissoes" -> pedirPermissoes(result)
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
                )
                if (cfg.phy !in setOf("1M", "2M", "CODED") || cfg.prioridade !in setOf("PADRAO", "ALTA") ||
                    cfg.repeticoes !in 1..50 || cfg.distancia < 0
                ) {
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
            else -> result.notImplemented()
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
    private var clienteDoServidor: BluetoothDevice? = null
    private var notificacaoLigada = false
    private var mtuServidor = 23
    private var remontagemServidor: QuadroSonda.Remontagem? = null
    private val conjuntos = mutableListOf<AdvertisingSetCallback>()

    // Envio por notificação (volta): fila e espera do aviso de envio.
    // Fila de notificações do servidor. O primeiro da fila está no ar enquanto
    // notificandoAgora; sai da fila no onNotificationSent. Quadro novo entra no fim: no
    // teste de 08/11/2026 o PEDIR_VOLTA chegou antes do aviso de envio do RESULTADO, e a
    // versão que recusava ("NOTIFICACAO_EM_ANDAMENTO") deixou a volta sem resposta.
    private val filaNotificacao = ArrayDeque<ByteArray>()
    private var notificandoAgora = false
    private var ocupadoNotificacao = 0
    private var voltaServidorN = 0
    private var voltaServidorComeco = 0L
    private var voltaServidorPedacos = 0
    private val tempoVoltaServidor = Runnable { falha("TEMPO_ESGOTADO", "papel=servidor etapa=volta n=$voltaServidorN"); esvaziarNotificacoes() }

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
                    if (clienteDoServidor != null && clienteDoServidor != device) {
                        // Um cliente por vez. Não grava quem era.
                        servidor?.cancelConnection(device)
                        medida("servidor", "evento=outro_cliente_recusado")
                        return@post
                    }
                    clienteDoServidor = device
                    notificacaoLigada = false
                    mtuServidor = 23
                    medida("servidor", "evento=conectou status=$status")
                    aoServidorConectado(device)
                } else if (newState == BluetoothProfile.STATE_DISCONNECTED && device == clienteDoServidor) {
                    clienteDoServidor = null
                    remontagemServidor = null
                    esvaziarNotificacoes()
                    h.removeCallbacks(tempoVoltaServidor)
                    medida("servidor", "evento=desconectou status=$status")
                }
            }
        }

        override fun onMtuChanged(device: BluetoothDevice, mtu: Int) {
            h.post { if (device == clienteDoServidor) mtuServidor = mtu }
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
            h.post { if (device == clienteDoServidor) notificacaoLigada = ligar }
        }

        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice, requestId: Int, characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean, responseNeeded: Boolean, offset: Int, value: ByteArray?,
        ) {
            if (responseNeeded) servidor?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, offset, null)
            val copia = value?.copyOf() ?: ByteArray(0)
            h.post { if (device == clienteDoServidor && characteristic.uuid == UUID_ENTRADA) entradaDoServidor(copia) }
        }

        override fun onNotificationSent(device: BluetoothDevice, status: Int) {
            h.post {
                if (device != clienteDoServidor) return@post
                if (status != BluetoothGatt.GATT_SUCCESS) {
                    falha("NOTIFICACAO", "papel=servidor status=$status n=$voltaServidorN")
                    esvaziarNotificacoes()
                    return@post
                }
                if (notificandoAgora && filaNotificacao.isNotEmpty()) filaNotificacao.removeFirst()
                notificandoAgora = false
                notificarProximo()
            }
        }
    }

    private fun entradaDoServidor(q: ByteArray) {
        when (QuadroSonda.tipo(q)) {
            QuadroSonda.TIPO_INICIO -> {
                val ini = QuadroSonda.lerInicio(q)
                if (ini == null) { falha("QUADRO_INVALIDO", "papel=servidor tipo=inicio"); return }
                // AMBOS: o outro escrevendo aqui enquanto este também é cliente = os dois conectaram.
                if (servidorModo == "AMBOS" && gatt != null && ini.n == 1) medida("ambos", "desfecho=duas_conexoes")
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
            QuadroSonda.TIPO_PEDIR_VOLTA -> {
                val p = QuadroSonda.lerPedirVolta(q)
                if (p == null) { falha("QUADRO_INVALIDO", "papel=servidor tipo=pedir_volta"); return }
                val dados = ByteArray(TAMANHO_CARTA).also { sorteio.nextBytes(it) }
                val tam = QuadroSonda.tamanhoDoPedaco(mtuServidor)
                val quadros = listOf(QuadroSonda.inicio(p.n, dados, tam)) + QuadroSonda.pedacos(dados, tam, p.sabotagem)
                voltaServidorN = p.n
                voltaServidorPedacos = QuadroSonda.quantosPedacos(TAMANHO_CARTA, tam)
                voltaServidorComeco = SystemClock.elapsedRealtime()
                h.removeCallbacks(tempoVoltaServidor)
                h.postDelayed(tempoVoltaServidor, TEMPO_TRANSFERENCIA_MS)
                notificar(quadros, envioDeVolta = true)
            }
            QuadroSonda.TIPO_RESULTADO -> {
                val r = QuadroSonda.lerResultado(q)
                if (r == null) { falha("QUADRO_INVALIDO", "papel=servidor tipo=resultado"); return }
                h.removeCallbacks(tempoVoltaServidor)
                val ms = SystemClock.elapsedRealtime() - voltaServidorComeco
                medida(
                    "servidor-enviou",
                    "n=${r.n} bytes=$TAMANHO_CARTA pedacos=$voltaServidorPedacos ms=$ms kbps=${kbps(TAMANHO_CARTA, ms)} " +
                        "resultado=${QuadroSonda.nomeDoCodigo(r.codigo)} ocupado=$ocupadoNotificacao mtu=$mtuServidor",
                )
            }
            else -> falha("QUADRO_INVALIDO", "papel=servidor tipo=desconhecido")
        }
    }

    private fun notificar(quadros: List<ByteArray>, envioDeVolta: Boolean) {
        if (!notificacaoLigada) { falha("SEM_NOTIFICACAO", "papel=servidor"); return }
        if (envioDeVolta) ocupadoNotificacao = 0
        filaNotificacao.addAll(quadros)
        if (!notificandoAgora) notificarProximo()
    }

    private fun esvaziarNotificacoes() {
        filaNotificacao.clear()
        notificandoAgora = false
    }

    private fun notificarProximo() {
        val g = servidor ?: return
        val d = clienteDoServidor ?: return
        val c = saidaServidor ?: return
        if (notificandoAgora || filaNotificacao.isEmpty()) return
        val r = g.notifyCharacteristicChanged(d, c, false, filaNotificacao.first())
        if (r == BluetoothStatusCodes.SUCCESS) { notificandoAgora = true; return } // espera o onNotificationSent
        ocupadoNotificacao++
        if (ocupadoNotificacao > TENTATIVAS_OCUPADO_MAX) {
            falha("NOTIFICACAO_RECUSADA", "papel=servidor codigo=$r")
            esvaziarNotificacoes()
            return
        }
        h.postDelayed({ notificarProximo() }, 2)
    }

    private fun aoServidorConectado(device: BluetoothDevice) {
        if (servidorModo != "AMBOS") return
        // O Android avisa o servidor também da conexão que este aparelho abriu como
        // cliente. Essa não conta como "o outro conectou primeiro".
        if (device == dispositivoAlvo && gatt != null) return
        // Quem recebeu a conexão antes de conectar fica de servidor.
        if (cliente != null && gatt == null) {
            medida("ambos", "desfecho=servidor")
            pararCliente()
        }
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
        remontagemServidor = null
        esvaziarNotificacoes()
        servidorPronto = false
    }

    // =====================================================================
    // CLIENTE
    // =====================================================================

    private data class Config(
        val modo: String, val phy: String, val prioridade: String,
        val distancia: Int, val repeticoes: Int, val calibrar: Boolean,
    )

    private sealed class Ev {
        data class Servicos(val status: Int) : Ev()
        data class Mtu(val mtu: Int, val status: Int) : Ev()
        data class Phy(val tx: Int, val rx: Int, val status: Int) : Ev()
        data class Descritor(val status: Int) : Ev()
        data class Escreveu(val status: Int) : Ev()
        data class Rssi(val rssi: Int, val status: Int) : Ev()
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

    private fun campos(c: Config) =
        "rodada=$rodada modo=${c.modo} distancia=${c.distancia} phy=${c.phy} prioridade=${c.prioridade} " +
            "calibrar=${sn(c.calibrar)}"

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
            val espera = sorteio.nextInt(ESPERA_AMBOS_MAX_MS + 1).toLong()
            etapa = "espera_ambos"
            h.postDelayed({ if (cliente === cfg && gatt == null) conectar(cfg, dispositivo) }, espera)
        } else {
            conectar(cfg, dispositivo)
        }
    }

    private fun conectar(cfg: Config, d: BluetoothDevice) {
        if (cfg.modo == "AMBOS" && clienteDoServidor != null) {
            medida("ambos", "desfecho=servidor")
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
                ida(g, cfg, 1)
            }
        }
    }

    private fun sabotagem(cfg: Config, n: Int): Int = when {
        !cfg.calibrar -> QuadroSonda.SABOTAGEM_NENHUMA
        n == 1 -> QuadroSonda.SABOTAGEM_TROCAR_BYTE
        n == 2 -> QuadroSonda.SABOTAGEM_PULAR_PEDACO
        else -> QuadroSonda.SABOTAGEM_NENHUMA
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

    // ---------- volta: o servidor notifica, o cliente confere ----------

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
        escrever(g, listOf(QuadroSonda.pedirVolta(n, sabotagem(cfg, n))), 0, IntArray(1)) { escritaFim = true; talvezSeguir() }
    }

    private fun terminarRodada(cfg: Config) {
        medida("rodada-fim", "${campos(cfg)} idas_ok=$idasOk/${cfg.repeticoes} voltas_ok=$voltasOk/${cfg.repeticoes}")
        if (cfg.calibrar) {
            val esperado = listOf("SHA_NAO_CONFERE", "PEDACO_FALTANDO") + List(cfg.repeticoes - 2) { "CONFERIU" }
            val ok = vistosIda == esperado && vistosVolta == esperado
            medida(
                "calibracao",
                "rodada=$rodada resultado=${if (ok) "OK" else "FALHOU"} ida=${vistosIda.joinToString(",")} volta=${vistosVolta.joinToString(",")}",
            )
        }
        pararCliente()
    }

    private fun falharCliente(codigo: String, extra: String = "") {
        val cfg = cliente
        if (cfg == null) { falha(codigo, extra); return }
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
        try {
            g?.disconnect()
            g?.close()
        } catch (e: Exception) {
            Log.e(TAG, "fechar cliente: ${e.javaClass.simpleName}")
        }
        if (servidor == null) ocupado(false)
    }

    // =====================================================================

    /** Para tudo: procura, anúncio, conexão e servidor. Também quando o app sai da frente. */
    fun parar(motivo: String) {
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
            linha.quitSafely()
        }
    }

    private fun kbps(bytes: Int, ms: Long): String =
        if (ms <= 0) "?" else String.format(Locale.ROOT, "%.1f", bytes * 8.0 / ms)
}
