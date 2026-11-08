package com.torionsystems.kryptos

import java.security.MessageDigest

/**
 * Quadros da sonda BLE (documentacao/arquitetura/modo-direto.md, passo 1). Kotlin puro, sem
 * nada do Android, para poder ser conferido fora do celular.
 *
 * Não é o protocolo do Kryptos: os bytes da sonda são aleatórios e servem só para medir.
 * A divisão em pedaços definitiva vai para o núcleo em Rust no passo 2.
 *
 * Cada valor escrito ou notificado tem no máximo 512 bytes: desde o Android 13 o
 * writeCharacteristic recusa valor maior que o máximo de um atributo (GATT_MAX_ATTR_LEN),
 * mesmo com MTU 517.
 *
 *   INICIO      = 1 | n (1) | total (4) | pedaços (2) | tamanho do pedaço (2) | SHA-256 (32)
 *   PEDACO      = 2 | índice (2) | dados
 *   RESULTADO   = 3 | n (1) | código (1) | recebidos (2) | primeiro faltando (2) | ms do receptor (4)
 *   PEDIR_VOLTA = 4 | n (1) | sabotagem (1)
 *
 * Inteiros em big-endian.
 */
object QuadroSonda {
    const val MAX_VALOR_ATRIBUTO = 512
    const val CABECALHO_PEDACO = 3
    const val TAMANHO_INICIO = 42
    const val TAMANHO_RESULTADO = 11
    const val TAMANHO_PEDIR_VOLTA = 3

    const val TIPO_INICIO: Byte = 1
    const val TIPO_PEDACO: Byte = 2
    const val TIPO_RESULTADO: Byte = 3
    const val TIPO_PEDIR_VOLTA: Byte = 4

    // Sabotagem: só para calibrar quem confere (regra 5.3).
    const val SABOTAGEM_NENHUMA = 0
    const val SABOTAGEM_TROCAR_BYTE = 1
    const val SABOTAGEM_PULAR_PEDACO = 2

    // Códigos do resultado.
    const val CONFERIU = 0
    const val SHA_NAO_CONFERE = 1
    const val PEDACO_FALTANDO = 2
    const val TAMANHO_ERRADO = 3
    const val INDICE_FORA = 4

    fun nomeDoCodigo(codigo: Int): String = when (codigo) {
        CONFERIU -> "CONFERIU"
        SHA_NAO_CONFERE -> "SHA_NAO_CONFERE"
        PEDACO_FALTANDO -> "PEDACO_FALTANDO"
        TAMANHO_ERRADO -> "TAMANHO_ERRADO"
        INDICE_FORA -> "INDICE_FORA"
        else -> "CODIGO_$codigo"
    }

    /** Dados por pedaço para um MTU: o menor entre MTU - 3 e 512, menos o cabeçalho. */
    fun tamanhoDoPedaco(mtu: Int): Int {
        require(mtu >= 23) { "MTU abaixo do mínimo do BLE" }
        return minOf(mtu - 3, MAX_VALOR_ATRIBUTO) - CABECALHO_PEDACO
    }

    fun quantosPedacos(total: Int, pedaco: Int): Int {
        require(total > 0 && pedaco > 0)
        return (total + pedaco - 1) / pedaco
    }

    fun sha256(b: ByteArray): ByteArray = MessageDigest.getInstance("SHA-256").digest(b)

    class Inicio(val n: Int, val total: Int, val pedacos: Int, val tamanhoPedaco: Int, val sha: ByteArray)

    class Resultado(val n: Int, val codigo: Int, val recebidos: Int, val primeiroFaltando: Int, val msReceptor: Long)

    class PedirVolta(val n: Int, val sabotagem: Int)

    // ---------- montar ----------

    fun inicio(n: Int, dados: ByteArray, tamanhoPedaco: Int): ByteArray {
        val pedacos = quantosPedacos(dados.size, tamanhoPedaco)
        require(n in 1..255 && pedacos <= 0xFFFF && tamanhoPedaco <= 0xFFFF)
        val b = ByteArray(TAMANHO_INICIO)
        b[0] = TIPO_INICIO
        b[1] = n.toByte()
        poe32(b, 2, dados.size.toLong())
        poe16(b, 6, pedacos)
        poe16(b, 8, tamanhoPedaco)
        sha256(dados).copyInto(b, 10)
        return b
    }

    /**
     * Os pedaços dos dados. A sabotagem age no pedaço do meio: troca o primeiro byte de
     * dados dele ou deixa ele de fora. O INICIO continua com o SHA dos dados certos.
     */
    fun pedacos(dados: ByteArray, tamanhoPedaco: Int, sabotagem: Int = SABOTAGEM_NENHUMA): List<ByteArray> {
        val total = quantosPedacos(dados.size, tamanhoPedaco)
        val meio = total / 2
        val lista = ArrayList<ByteArray>(total)
        for (i in 0 until total) {
            if (sabotagem == SABOTAGEM_PULAR_PEDACO && i == meio && total > 1) continue
            val de = i * tamanhoPedaco
            val ate = minOf(de + tamanhoPedaco, dados.size)
            val q = ByteArray(CABECALHO_PEDACO + (ate - de))
            q[0] = TIPO_PEDACO
            poe16(q, 1, i)
            dados.copyInto(q, CABECALHO_PEDACO, de, ate)
            if (sabotagem == SABOTAGEM_TROCAR_BYTE && i == meio) {
                q[CABECALHO_PEDACO] = (q[CABECALHO_PEDACO].toInt() xor 0x01).toByte()
            }
            lista.add(q)
        }
        return lista
    }

    fun resultado(r: Resultado): ByteArray {
        val b = ByteArray(TAMANHO_RESULTADO)
        b[0] = TIPO_RESULTADO
        b[1] = r.n.toByte()
        b[2] = r.codigo.toByte()
        poe16(b, 3, r.recebidos)
        poe16(b, 5, r.primeiroFaltando)
        poe32(b, 7, r.msReceptor.coerceIn(0, 0xFFFFFFFFL))
        return b
    }

    fun pedirVolta(n: Int, sabotagem: Int): ByteArray =
        byteArrayOf(TIPO_PEDIR_VOLTA, n.toByte(), sabotagem.toByte())

    // ---------- ler (null = fora do formato) ----------

    fun tipo(b: ByteArray): Byte? = if (b.isEmpty()) null else b[0]

    fun lerInicio(b: ByteArray): Inicio? {
        if (b.size != TAMANHO_INICIO || b[0] != TIPO_INICIO) return null
        val n = b[1].toInt() and 0xFF
        val total = le32(b, 2)
        val pedacos = le16(b, 6)
        val tamanho = le16(b, 8)
        if (n == 0 || total <= 0 || total > Int.MAX_VALUE || tamanho == 0 || tamanho > MAX_VALOR_ATRIBUTO - CABECALHO_PEDACO) return null
        if (pedacos != quantosPedacos(total.toInt(), tamanho)) return null
        return Inicio(n, total.toInt(), pedacos, tamanho, b.copyOfRange(10, 42))
    }

    fun lerResultado(b: ByteArray): Resultado? {
        if (b.size != TAMANHO_RESULTADO || b[0] != TIPO_RESULTADO) return null
        return Resultado(b[1].toInt() and 0xFF, b[2].toInt() and 0xFF, le16(b, 3), le16(b, 5), le32(b, 7))
    }

    fun lerPedirVolta(b: ByteArray): PedirVolta? {
        if (b.size != TAMANHO_PEDIR_VOLTA || b[0] != TIPO_PEDIR_VOLTA) return null
        val s = b[2].toInt() and 0xFF
        if (s > SABOTAGEM_PULAR_PEDACO) return null
        return PedirVolta(b[1].toInt() and 0xFF, s)
    }

    /**
     * Remonta uma transferência a partir do INICIO. Devolve o resultado quando chega o
     * último pedaço, quando todos chegaram ou quando um pedaço vem fora do formato; antes
     * disso, null. Depois do resultado, ignora o que chegar.
     *
     * O GATT entrega em ordem numa mesma conexão; o último pedaço chegando com falta de
     * algum antes dele quer dizer pedaço perdido.
     */
    class Remontagem(val inicio: Inicio, private val agoraMs: () -> Long) {
        private val dados = ByteArray(inicio.total)
        private val chegou = BooleanArray(inicio.pedacos)
        private val comeco = agoraMs()
        var recebidos = 0
            private set
        var repetidos = 0
            private set
        var terminou = false
            private set

        fun receber(q: ByteArray): Resultado? {
            if (terminou) return null
            if (q.size < CABECALHO_PEDACO || q[0] != TIPO_PEDACO) return fim(TAMANHO_ERRADO)
            val i = le16(q, 1)
            if (i >= inicio.pedacos) return fim(INDICE_FORA)
            val esperado = if (i < inicio.pedacos - 1) inicio.tamanhoPedaco
            else inicio.total - inicio.tamanhoPedaco * (inicio.pedacos - 1)
            if (q.size - CABECALHO_PEDACO != esperado) return fim(TAMANHO_ERRADO)
            if (chegou[i]) {
                repetidos++
            } else {
                chegou[i] = true
                recebidos++
                q.copyInto(dados, i * inicio.tamanhoPedaco, CABECALHO_PEDACO, q.size)
            }
            if (i == inicio.pedacos - 1 || recebidos == inicio.pedacos) {
                if (recebidos < inicio.pedacos) return fim(PEDACO_FALTANDO)
                return fim(if (sha256(dados).contentEquals(inicio.sha)) CONFERIU else SHA_NAO_CONFERE)
            }
            return null
        }

        private fun fim(codigo: Int): Resultado {
            terminou = true
            val faltando = chegou.indexOfFirst { !it }.let { if (it < 0) 0xFFFF else it }
            return Resultado(inicio.n, codigo, recebidos, faltando, agoraMs() - comeco)
        }
    }

    // ---------- inteiros ----------

    private fun poe16(b: ByteArray, p: Int, v: Int) {
        b[p] = (v ushr 8).toByte(); b[p + 1] = v.toByte()
    }

    private fun poe32(b: ByteArray, p: Int, v: Long) {
        b[p] = (v ushr 24).toByte(); b[p + 1] = (v ushr 16).toByte()
        b[p + 2] = (v ushr 8).toByte(); b[p + 3] = v.toByte()
    }

    private fun le16(b: ByteArray, p: Int): Int = ((b[p].toInt() and 0xFF) shl 8) or (b[p + 1].toInt() and 0xFF)

    private fun le32(b: ByteArray, p: Int): Long =
        ((b[p].toLong() and 0xFF) shl 24) or ((b[p + 1].toLong() and 0xFF) shl 16) or
            ((b[p + 2].toLong() and 0xFF) shl 8) or (b[p + 3].toLong() and 0xFF)
}
