# ferramentas/sonda-roteiro.ps1
# Conduz a sonda BLE (documentacao/arquitetura/modo-direto.md) pelo cabo, sem tocar na
# tela. Quem chama e o instalar-no-celular.ps1 -SondaBle com -Conferir, -Autonomo ou
# -Alcance, depois de instalar e abrir o app; nao precisa rodar este direto.
#
# Como fala com a sonda: grava uma linha "id=<n> acao=<acao> chave=valor ..." em
# files/sonda-comando.tmp, na pasta privada do app, pelo run-as (so app de debug, so pelo
# cabo; nenhum outro app escreve ali), e renomeia para files/sonda-comando.txt. A sonda le
# a cada 250 ms, marca "comando id=<n>" ao receber e "comando-fim id=<n> desfecho=..." ao
# terminar. O que foi gravado e provado pela resposta da sonda, que repete o id e a acao.
# Duas formas de gravar, a primeira que funcionar no aparelho:
#   exec-in: adb exec-in run-as <pacote> tee files/sonda-comando.tmp (texto pela entrada);
#   push:    adb push para /data/local/tmp e run-as cp de la.
#
# Modos:
#   Conferir  um celular por vez: o comando chega, L2CAP abre escuta, anuncia, recusa
#             comando errado, para, carrega e cancela roteiro, procura sem o outro
#             (NAO_ACHOU_30S), recusa comando com o app fora da frente e aceita de volta.
#   UmMetro   dois celulares a 1 m: calibra e mede os tres caminhos de volta nos dois
#             sentidos, AMBOS 3 vezes e o ciclo de vida (botao de inicio no meio das idas).
#   Ambos     dois celulares a 1 m: uma calibracao e 10 rodadas de AMBOS (2 idas e voltas
#             cada): 3 com a espera aleatoria normal, 3 com espera 0 (os dois conectam
#             juntos) e 4 com espera 0 e o OLA segurado 2 s no servidor (os dois clientes
#             mandam o OLA antes de qualquer decisao: desempate pela comparacao do nonce).
#   Coded     dois celulares a 1 m: uma calibracao e, nos dois sentidos, 3 rodadas em
#             CODED (conecta em Coded) e 3 em CODED1M (conecta em 1M e troca para Coded),
#             1 ida e volta cada (10/11/2026: o Edge procurando nao abria conexao em Coded).
#   Alcance   dois celulares: o primeiro anuncia, o segundo calibra (sem -SemCalibrar) e
#             recebe o roteiro de distancias; depois a pessoa tira os cabos e toca PROXIMO
#             PONTO em cada distancia.
#
# Saida: linhas OK / AVISO / FALHA e RESULTADO: OK ou FALHOU. Rodada que falhou no
# Bluetooth (NAO_ACHOU_30S, CONEXAO_CAIU...) sai como AVISO: e medida, nao defeito do
# roteiro. FALHA e o roteiro que nao andou (sonda que nao responde, comando sem fim).
#
# KRYPTOS_SONDA_ESCALA (so para a calibracao com o adb falso): multiplica todas as esperas.
# Fora de 1, o resultado avisa.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateSet('Conferir', 'UmMetro', 'Coded', 'Ambos', 'Alcance')][string]$Modo,
    [Parameter(Mandatory = $true)][string]$Adb,
    [Parameter(Mandatory = $true)][string[]]$Seriais,
    [string[]]$Modelos = @(),
    [string]$Pacote = 'com.torionsystems.kryptos',
    [string]$Distancias = '5,10,15,20,30',
    [switch]$SemCalibrar
)

$ErrorActionPreference = 'Stop'
$resultados = New-Object System.Collections.Generic.List[string]
# Chamado de outro script com -File, a lista chega como um texto so, separado por virgula.
$Seriais = @($Seriais | ForEach-Object { $_ -split ',' } | Where-Object { $_.Trim() } | ForEach-Object { $_.Trim() })
$Modelos = @($Modelos | ForEach-Object { $_ -split ',' } | Where-Object { $_.Trim() } | ForEach-Object { $_.Trim() })
$Escala = 1.0
if ($env:KRYPTOS_SONDA_ESCALA) { $Escala = [double]::Parse($env:KRYPTOS_SONDA_ESCALA, [Globalization.CultureInfo]::InvariantCulture) }
$medidas = 'files/sonda-ble.txt'
$comandoTmp = 'files/sonda-comando.tmp'
$comandoArq = 'files/sonda-comando.txt'
$remotoPush = '/data/local/tmp/kryptos-sonda-comando.txt'
$como = @{}
$script:proximoId = [int](Get-Date -Format 'HHmm') * 100

# Rodada a 1 m: calibracao de cada caminho e as medidas (modo-direto.md, decisao 9).
$rodadas1m = @(
    'acao=calibrar phy=2M prioridade=ALTA volta=NOTIFICACAO',
    'acao=calibrar phy=2M prioridade=ALTA volta=ESCRITA',
    'acao=calibrar phy=2M prioridade=ALTA volta=L2CAP',
    'acao=procurar phy=1M prioridade=PADRAO volta=NOTIFICACAO',
    'acao=procurar phy=2M prioridade=ALTA volta=NOTIFICACAO',
    'acao=procurar phy=2M prioridade=ALTA volta=ESCRITA',
    'acao=procurar phy=2M prioridade=ALTA volta=L2CAP',
    # Coded: 2 idas e voltas. No sentido lento a volta leva minutos (estimo 130 a 160 s) e a
    # rodada inteira precisa caber nos 420 s da espera do Comando.
    'acao=procurar phy=CODED prioridade=ALTA volta=NOTIFICACAO repeticoes=2'
)
$combosAlcance = '1M-PADRAO-NOTIFICACAO,CODED-ALTA-NOTIFICACAO'

function Nome([string]$s) {
    $i = [array]::IndexOf($Seriais, $s)
    if ($i -ge 0 -and $i -lt $Modelos.Count) { return $Modelos[$i] }
    return $s
}

function Pausa([double]$ms) { Start-Sleep -Milliseconds ([int][math]::Max(1, $ms * $Escala)) }

# adb com o serial; saida diferente de 0 vira erro com a saida junto.
function AdbS([string]$s, [string[]]$a) {
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & $Adb -s $s @a 2>&1 } finally { $ErrorActionPreference = $antes }
    $texto = (@($saida | ForEach-Object { "$_" }) -join "`n").Trim()
    if ($LASTEXITCODE -ne 0) { throw "adb $($a -join ' ') saiu com $LASTEXITCODE`: $texto" }
    return $texto
}

function Medidas([string]$s) {
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $t = @(& $Adb -s $s exec-out run-as $Pacote cat $medidas 2>$null) } finally { $ErrorActionPreference = $antes }
    if ($LASTEXITCODE -ne 0) { return @() }
    return @($t | ForEach-Object { "$_" } | Where-Object { $_ -match '\S' })
}

function EsperarLinha([string]$s, [string]$padrao, [int]$desde, [double]$segundos) {
    $limite = (Get-Date).AddSeconds($segundos * $Escala)
    do {
        # @(): com uma linha so, o PowerShell devolveria o texto, e $l[0] seria uma letra.
        $l = @(Medidas $s)
        for ($i = $desde; $i -lt $l.Count; $i++) { if ($l[$i] -match $padrao) { return $l[$i] } }
        Pausa 1500
    } while ((Get-Date) -lt $limite)
    return $null
}

function Gravar([string]$s, [string]$texto, [string]$forma) {
    if ($forma -eq 'exec-in') {
        $antes = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try { $saida = @($texto | & $Adb -s $s exec-in run-as $Pacote tee $comandoTmp 2>&1) } finally { $ErrorActionPreference = $antes }
        if ($LASTEXITCODE -ne 0) { throw "exec-in saiu com $LASTEXITCODE`: $((@($saida | ForEach-Object { "$_" })) -join ' ')" }
    } else {
        $local = [System.IO.Path]::GetTempFileName()
        try {
            [System.IO.File]::WriteAllText($local, $texto + "`n", (New-Object System.Text.ASCIIEncoding))
            AdbS $s @('push', $local, $remotoPush) | Out-Null
            AdbS $s @('shell', 'run-as', $Pacote, 'cp', $remotoPush, $comandoTmp) | Out-Null
        } finally {
            Remove-Item -LiteralPath $local -Force -ErrorAction SilentlyContinue
            try { AdbS $s @('shell', 'rm', '-f', $remotoPush) | Out-Null } catch { Write-Host "   aviso: nao apaguei $remotoPush no $(Nome $s)" }
        }
    }
}

# Renomear e o gatilho: a sonda so le o .txt, nunca o .tmp pela metade.
function Soltar([string]$s) { AdbS $s @('shell', 'run-as', $Pacote, 'mv', $comandoTmp, $comandoArq) | Out-Null }

function Campo([string]$linha, [string]$nome) {
    if ($linha -match "(?:^|\s)$nome=(\S+)") { return $Matches[1] }
    return ''
}

# Manda e espera a sonda receber. Devolve id e a posicao no arquivo antes do comando.
function Mandar([string]$s, [string]$texto, [switch]$SemSoltar) {
    $id = ++$script:proximoId
    $desde = @(Medidas $s).Count
    Gravar $s "id=$id $texto" $como[$s]
    $p = [pscustomobject]@{ Serial = $s; Id = $id; Desde = $desde; Texto = $texto }
    if (-not $SemSoltar) { Receber $p }
    return $p
}

function Receber($p) {
    Soltar $p.Serial
    $ack = EsperarLinha $p.Serial "sonda-ble (comando id=$($p.Id) acao=|COMANDO_INVALIDO id=$($p.Id) )" $p.Desde 15
    if (-not $ack) { throw "a sonda do $(Nome $p.Serial) nao respondeu ao comando '$($p.Texto)' em 15 s (o arquivo de comando chegou?)" }
}

function EsperarFim($p, [double]$segundos) {
    $fim = EsperarLinha $p.Serial "sonda-ble comando-fim id=$($p.Id) desfecho=" $p.Desde $segundos
    if (-not $fim) { throw "a sonda do $(Nome $p.Serial) nao terminou '$($p.Texto)' em $segundos s" }
    return [pscustomobject]@{ Desfecho = (Campo $fim 'desfecho'); Codigo = (Campo $fim 'codigo'); Desde = $p.Desde; Linha = $fim }
}

function Comando([string]$s, [string]$texto, [double]$segundos = 30) {
    $p = Mandar $s $texto
    return EsperarFim $p $segundos
}

# Descobre por onde o comando chega neste aparelho, com um inventario.
function PrepararCaminho([string]$s) {
    $erros = @()
    foreach ($forma in @('exec-in', 'push')) {
        $como[$s] = $forma
        try {
            $r = Comando $s 'acao=inventario' 20
            if ($r.Desfecho -eq 'ok') { return $forma }
            $erros += "$forma`: desfecho=$($r.Desfecho) $($r.Codigo)"
        } catch {
            $erros += "$forma`: $(($_.Exception.Message -split "`n")[0])"
        }
    }
    throw "nenhuma forma levou o comando ate a sonda do $(Nome $s): $($erros -join ' | ')"
}

function EsperarApp([string]$s) {
    # A tela da sonda pede o inventario quando abre com a permissao dada: e quando ela
    # passa a ler comandos.
    if (-not (EsperarLinha $s 'MEDIDA sonda-ble aparelho ' 0 40)) {
        throw "a tela da sonda do $(Nome $s) nao abriu em 40 s (nenhuma linha 'aparelho'; a permissao foi dada?)"
    }
}

function PararTodos([string[]]$lista) {
    foreach ($s in $lista) {
        $r = Comando $s 'acao=parar' 30
        if ($r.Desfecho -ne 'ok') { throw "parar no $(Nome $s): desfecho=$($r.Desfecho)" }
    }
}

function Anunciar([string]$s) {
    $r = Comando $s 'acao=anunciar' 20
    if ($r.Desfecho -ne 'ok') { throw "anunciar no $(Nome $s): desfecho=$($r.Desfecho) $($r.Codigo)" }
    if (-not (EsperarLinha $s 'sonda-ble anuncio tipo=legado resultado=ok' $r.Desde 15)) { throw "o $(Nome $s) nao confirmou o anuncio legado em 15 s" }
    return $r
}

# Uma rodada de cliente. Falha do Bluetooth e medida (AVISO); falha do roteiro e FALHA.
function Rodada([string]$cli, [string]$texto, [string]$rotulo) {
    $r = Comando $cli "$texto distancia=1" 420
    if ($r.Desfecho -eq 'nao_comecou' -and $r.Codigo -eq 'PROCURA_DEMAIS') {
        Pausa 12000
        $r = Comando $cli "$texto distancia=1" 420
    }
    $marca = if ($r.Desfecho -eq 'ok' -or $r.Desfecho -eq 'calibracao_OK') { 'OK   ' } else { 'AVISO' }
    $cod = if ($r.Codigo) { " $($r.Codigo)" } else { '' }
    $resultados.Add("$marca $rotulo, $(Nome $cli) procura: $(($texto -replace 'acao=', '') -replace ' (phy|prioridade|volta)=', ' ') -> $($r.Desfecho)$cod")
    Write-Host "   $rotulo $texto -> $($r.Desfecho)$cod"
    Pausa 6500
    return $r
}

# Rodadas de AMBOS: os dois recebem o comando e so depois sao soltos, um logo depois do
# outro. Com espera_ms=0 nenhum espera antes de conectar: os dois conectam juntos. Com
# atraso_ola_ms, o servidor segura o OLA e os dois clientes chegam a se apresentar.
# Esperado em todas: um 'ok' (o cliente que seguiu) e um 'servidor'.
# Colisao (09/11/2026): os dois abriram conexao um no outro. Dois jeitos de resolver:
#   - antes do OLA: o cliente que ainda nao tinha mandado o OLA desiste (no log, o mesmo
#     celular marca 'ambos desfecho=cliente' e depois 'ambos desfecho=servidor');
#   - pelo nonce: os dois ja tinham mandado; 'ambos desfecho=colisao'.
#   - linha cruzada (09/11/2026, rodadas 8 e 9): o Android de um recusa o enlace duplicado
#     e um lado cai (0x3e, 133) ou espera os servicos a toa; a sonda fecha, fica de servidor
#     e tenta de novo uma vez ('ambos recuperacao=inicio' e 'recuperacao=nova_tentativa').
# Classificacao: um 'ok' e o outro 'servidor' = OK; um 'ok' e o outro 'falha' = AVISO (o
# outro serviu, mas falhou como cliente); nenhum 'ok' = FALHA; dois 'ok' = FALHA (o
# desempate nao segurou).
function RodadasAmbos([string]$a, [string]$b, [int]$normais, [int]$forcadas, [int]$comAtraso = 0, [int]$repeticoes = 5) {
    $boas = 0; $antesDoOla = 0; $peloNonce = 0; $cruzadas = 0; $recuperadas = 0; $nenhum = 0; $dois = 0; $avisos = 0
    $total = $normais + $forcadas + $comAtraso
    for ($k = 1; $k -le $total; $k++) {
        $forcada = $k -gt $normais
        $atrasada = $k -gt ($normais + $forcadas)
        PararTodos @($a, $b)
        Pausa 6500
        $texto = "acao=ambos phy=1M prioridade=PADRAO volta=NOTIFICACAO distancia=1 repeticoes=$repeticoes" + $(if ($forcada) { ' espera_ms=0' } else { '' }) + $(if ($atrasada) { ' atraso_ola_ms=2000' } else { '' })
        $pa = Mandar $a $texto -SemSoltar
        $pb = Mandar $b $texto -SemSoltar
        $t0 = Get-Date
        Soltar $a; Soltar $b
        $msSoltar = [math]::Round(((Get-Date) - $t0).TotalMilliseconds)
        $fa = EsperarFim $pa 420
        $fb = EsperarFim $pb 420
        $linhasA = @(@(Medidas $a) | Select-Object -Skip $pa.Desde)
        $linhasB = @(@(Medidas $b) | Select-Object -Skip $pb.Desde)
        $nonce = @($linhasA + $linhasB | Where-Object { $_ -match 'sonda-ble ambos desfecho=colisao' }).Count -gt 0
        $nRec = @($linhasA + $linhasB | Where-Object { $_ -match 'sonda-ble ambos recuperacao=inicio' }).Count
        $desistiu = $false
        foreach ($ls in @(, $linhasA) + @(, $linhasB)) {
            $iCli = -1; $iSrv = -1
            for ($i = 0; $i -lt $ls.Count; $i++) {
                if ($ls[$i] -match 'sonda-ble ambos desfecho=cliente') { $iCli = $i }
                if ($ls[$i] -match 'sonda-ble ambos desfecho=servidor' -and $iCli -ge 0) { $iSrv = $i }
            }
            if ($iCli -ge 0 -and $iSrv -gt $iCli) { $desistiu = $true }
        }
        $col = if ($nonce) { 'pelo nonce' } elseif ($nRec -gt 0) { "linha cruzada ($nRec recuperacao)" } elseif ($desistiu) { 'antes do OLA' } else { 'nao' }
        if ($nonce) { $peloNonce++ } elseif ($nRec -gt 0) { $cruzadas++ } elseif ($desistiu) { $antesDoOla++ }
        $nOk = @(@($fa.Desfecho, $fb.Desfecho) | Where-Object { $_ -eq 'ok' }).Count
        $outro = if ($fa.Desfecho -eq 'ok') { $fb } else { $fa }
        if ($nOk -eq 2) { $marca = 'FALHA'; $dois++; $nota = '; DOIS CLIENTES (o desempate nao segurou)' }
        elseif ($nOk -eq 0) { $marca = 'FALHA'; $nenhum++; $nota = '; NENHUM CLIENTE (nada foi entregue)' }
        elseif ($outro.Desfecho -eq 'servidor') { $marca = 'OK   '; $boas++; $nota = '' }
        else { $marca = 'AVISO'; $avisos++; $nota = "; um cliente so, mas o outro terminou em $($outro.Desfecho) em vez de servidor" }
        if ($nRec -gt 0 -and $nOk -eq 1) { $recuperadas++ }
        $tipo = if ($atrasada) { 'espera 0 e OLA segurado 2 s' } elseif ($forcada) { 'espera 0 (colisao provocada)' } else { 'espera aleatoria' }
        $resultados.Add("$marca AMBOS $k, $tipo`: $(Nome $a)=$($fa.Desfecho)$(if ($fa.Codigo) { " $($fa.Codigo)" }), $(Nome $b)=$($fb.Desfecho)$(if ($fb.Codigo) { " $($fb.Codigo)" }); colisao: $col; soltos com $msSoltar ms de diferenca$nota")
        Write-Host "   AMBOS $k ($tipo): $($fa.Desfecho) / $($fb.Desfecho), colisao: $col"
    }
    $marca = if ($nenhum -gt 0 -or $dois -gt 0) { 'FALHA' } elseif ($boas -eq $total) { 'OK   ' } else { 'AVISO' }
    $resultados.Add("$marca AMBOS: $boas de $total com um cliente so; $avisos com aviso; $nenhum sem nenhum cliente; $dois com dois clientes; colisoes: $($antesDoOla + $peloNonce + $cruzadas) ($antesDoOla antes do OLA, $peloNonce pelo nonce, $cruzadas linha cruzada; $recuperadas recuperada(s))")
}

$ok = $false
try {
    if ($Escala -ne 1.0) { $resultados.Add("AVISO esperas multiplicadas por $Escala (KRYPTOS_SONDA_ESCALA): so para a calibracao") }
    if ($Modo -eq 'Conferir' -and $Seriais.Count -lt 1) { throw 'Conferir precisa de pelo menos um celular' }
    if ($Modo -ne 'Conferir' -and $Seriais.Count -ne 2) { throw "$Modo precisa de dois celulares; vieram $($Seriais.Count)" }

    foreach ($s in $Seriais) {
        Write-Host ".. $(Nome $s): esperando a tela da sonda"
        EsperarApp $s
        $forma = PrepararCaminho $s
        $resultados.Add("OK    $(Nome $s): o comando chega a sonda por $forma")
        # O app e quem diz se o adaptador esta ligado (BluetoothAdapter.isEnabled, no
        # inventario que PrepararCaminho acabou de pedir). Desligado, nada adiante mede:
        # para aqui, com o motivo, em vez de cada rodada dar BLUETOOTH_DESLIGADO.
        $inv = @(Medidas $s | Where-Object { $_ -match 'sonda-ble aparelho ' }) | Select-Object -Last 1
        if ($inv -match 'bluetooth_ligado=nao') { throw "o app do $(Nome $s) ve o Bluetooth DESLIGADO (inventario: bluetooth_ligado=nao): ligue o Bluetooth no celular e rode de novo" }
        if ($inv -notmatch 'bluetooth_ligado=sim') { throw "nao consegui conferir pelo app se o Bluetooth do $(Nome $s) esta ligado (sem bluetooth_ligado no inventario)" }
        $resultados.Add("OK    $(Nome $s): o app ve o Bluetooth ligado (inventario)")
    }

    if ($Modo -eq 'Conferir') {
        foreach ($s in $Seriais) {
            $n = Nome $s
            Write-Host ".. $n`: conferindo a sonda sozinha"

            $r = Comando $s 'acao=l2cap-teste' 20
            $l = EsperarLinha $s 'sonda-ble l2cap-teste ' $r.Desde 5
            if ($r.Desfecho -ne 'ok') { throw "$n`: L2CAP nao abriu escuta: desfecho=$($r.Desfecho) $($r.Codigo) $l" }
            $resultados.Add("OK    $n`: L2CAP abre escuta sem pareamento (psm $(Campo $l 'psm'), faixa dinamica do BLE)")

            $r = Anunciar $s
            $coded = EsperarLinha $s 'sonda-ble (anuncio tipo=coded|FALHA sonda-ble ANUNCIO_CODED)' $r.Desde 10
            $estadoCoded = if ($coded -match 'resultado=ok') { 'Coded ok' } elseif ($coded -match 'SEM_SUPORTE') { 'Coded sem suporte' } elseif ($coded) { "Coded: $((($coded -split 'sonda-ble ')[1]))" } else { 'Coded sem resposta' }
            $resultados.Add("OK    $n`: anuncia (legado ok, $estadoCoded) e o servidor GATT abre")

            $r = Comando $s 'acao=voar' 15
            if ($r.Desfecho -ne 'recusado' -or -not (EsperarLinha $s "COMANDO_INVALIDO id=\d+ motivo=acao" $r.Desde 5)) { throw "$n`: acao desconhecida nao foi recusada (desfecho=$($r.Desfecho))" }
            $r = Comando $s 'acao=calibrar repeticoes=2' 15
            if ($r.Desfecho -ne 'recusado') { throw "$n`: calibracao com 2 repeticoes nao foi recusada (desfecho=$($r.Desfecho))" }
            $resultados.Add("OK    $n`: recusa comando errado (acao desconhecida; calibracao com menos de 3)")

            $r = Comando $s 'acao=parar' 15
            if ($r.Desfecho -ne 'ok' -or -not (EsperarLinha $s 'sonda-ble parou motivo=PEDIDO' $r.Desde 5)) { throw "$n`: parar nao parou o anuncio" }
            $resultados.Add("OK    $n`: para pelo comando")

            $r = Comando $s 'acao=roteiro distancias=5,10 combos=1M-PADRAO-NOTIFICACAO repeticoes=3' 15
            if ($r.Desfecho -ne 'ok' -or -not (EsperarLinha $s 'sonda-ble roteiro evento=carregado' $r.Desde 5)) { throw "$n`: o roteiro nao carregou (desfecho=$($r.Desfecho))" }
            $r = Comando $s 'acao=parar' 15
            if (-not (EsperarLinha $s 'sonda-ble roteiro-fim motivo=PEDIDO' $r.Desde 5)) { throw "$n`: parar nao cancelou o roteiro" }
            $resultados.Add("OK    $n`: roteiro de alcance carrega e sai no parar")

            Write-Host "   procura sem o outro celular (espera ate 30 s)"
            $inicio = Get-Date
            $r = Comando $s 'acao=procurar phy=1M prioridade=PADRAO volta=NOTIFICACAO' 60
            if ($r.Desfecho -ne 'falha' -or $r.Codigo -ne 'NAO_ACHOU_30S') { throw "$n`: procurar sozinho deveria dar NAO_ACHOU_30S; deu $($r.Desfecho) $($r.Codigo)" }
            $resultados.Add("OK    $n`: procura sem localizacao, sem o outro celular: NAO_ACHOU_30S em $([math]::Round(((Get-Date) - $inicio).TotalSeconds)) s, fim marcado")

            AdbS $s @('shell', 'input', 'keyevent', 'KEYCODE_HOME') | Out-Null
            Pausa 2000
            $r = Comando $s 'acao=inventario' 15
            if ($r.Desfecho -ne 'recusado' -or $r.Codigo -ne 'COMANDO_FORA_DA_FRENTE') { throw "$n`: com o app fora da frente o comando deveria ser recusado; deu $($r.Desfecho) $($r.Codigo)" }
            AdbS $s @('shell', 'am', 'start', '-n', "$Pacote/.MainActivity") | Out-Null
            Pausa 3000
            $r = Comando $s 'acao=inventario' 15
            if ($r.Desfecho -ne 'ok') { throw "$n`: de volta a frente, o comando deveria valer; deu $($r.Desfecho) $($r.Codigo)" }
            $resultados.Add("OK    $n`: fora da frente o comando e recusado (COMANDO_FORA_DA_FRENTE); de volta, aceito")
        }
    }

    if ($Modo -eq 'UmMetro') {
        $a = $Seriais[0]; $b = $Seriais[1]
        foreach ($par in @(@($a, $b), @($b, $a))) {
            $srv = $par[0]; $cli = $par[1]
            Write-Host ".. $(Nome $srv) anuncia, $(Nome $cli) procura (1 m)"
            PararTodos @($a, $b)
            Anunciar $srv | Out-Null
            foreach ($t in $rodadas1m) { Rodada $cli $t "1 m, $(Nome $srv) anuncia" | Out-Null }
        }

        Write-Host '.. AMBOS, os dois juntos: 3 com espera aleatoria e 3 com espera 0'
        RodadasAmbos $a $b 3 3
        PararTodos @($a, $b)
        Pausa 6500

        Write-Host '.. ciclo de vida: botao de inicio no meio das idas'
        # A rodada pode acabar antes da primeira ida por falha do Bluetooth (09/11/2026:
        # CONEXAO_CAIU 133 logo depois do AMBOS). Isso e medida, nao defeito do roteiro:
        # tenta de novo uma vez; se repetir, o ciclo de vida fica sem teste (AVISO).
        $chegou = $false
        $motivos = @()
        for ($tentativa = 1; $tentativa -le 2 -and -not $chegou; $tentativa++) {
            if ($tentativa -gt 1) { PararTodos @($a, $b); Pausa 6500 }
            $r = Anunciar $a
            $desdeA = $r.Desde
            $p = Mandar $b 'acao=procurar phy=1M prioridade=PADRAO volta=NOTIFICACAO repeticoes=20 distancia=1'
            $l = EsperarLinha $b "(MEDIDA sonda-ble ida |sonda-ble comando-fim id=$($p.Id) )" $p.Desde 90
            if (-not $l) { throw "a rodada do ciclo de vida nao chegou a primeira ida nem terminou em 90 s no $(Nome $b)" }
            if ($l -match 'comando-fim') { $motivos += "$(Campo $l 'desfecho') $(Campo $l 'codigo')".Trim() } else { $chegou = $true }
        }
    }
    if ($Modo -eq 'UmMetro' -and -not $chegou) {
        $resultados.Add("AVISO ciclo de vida sem teste: nas 2 tentativas a rodada acabou antes da primeira ida ($($motivos -join '; '))")
        PararTodos @($a, $b)
    }
    if ($Modo -eq 'UmMetro' -and $chegou) {
        AdbS $b @('shell', 'input', 'keyevent', 'KEYCODE_HOME') | Out-Null
        $saiu = EsperarLinha $b 'FALHA sonda-ble APP_SAIU_DA_FRENTE' $p.Desde 20
        $caiu = EsperarLinha $a 'sonda-ble servidor evento=desconectou' $desdeA 30
        $f = EsperarFim $p 30
        AdbS $b @('shell', 'am', 'start', '-n', "$Pacote/.MainActivity") | Out-Null
        Pausa 3000
        $volta = Comando $b 'acao=inventario' 15
        if (-not $saiu -or -not $caiu -or $f.Codigo -ne 'APP_SAIU_DA_FRENTE' -or $volta.Desfecho -ne 'ok') {
            $resultados.Add("AVISO ciclo de vida: saiu da frente=$([bool]$saiu), o outro viu desconectar=$([bool]$caiu), fim=$($f.Desfecho) $($f.Codigo), de volta=$($volta.Desfecho)")
        } else {
            $resultados.Add("OK    ciclo de vida: $(Nome $b) parou ao sair da frente, $(Nome $a) viu a desconexao, e de volta a sonda aceita comando")
        }
        PararTodos @($a, $b)
    }

    if ($Modo -eq 'Coded') {
        $a = $Seriais[0]; $b = $Seriais[1]
        Write-Host '.. calibracao a 1 m antes do Coded'
        PararTodos @($a, $b)
        Anunciar $a | Out-Null
        $r = Rodada $b 'acao=calibrar phy=1M prioridade=PADRAO volta=NOTIFICACAO' '1 m, antes do Coded'
        if ($r.Desfecho -ne 'calibracao_OK') { throw "a calibracao antes do Coded nao deu OK ($($r.Desfecho) $($r.Codigo))" }
        foreach ($par in @(@($a, $b), @($b, $a))) {
            $srv = $par[0]; $cli = $par[1]
            Write-Host ".. Coded: $(Nome $srv) anuncia, $(Nome $cli) procura (1 m)"
            PararTodos @($a, $b)
            Anunciar $srv | Out-Null
            foreach ($phy in @('CODED', 'CODED1M')) {
                $okCoded = 0; $codigos = @()
                for ($k = 1; $k -le 3; $k++) {
                    $r = Rodada $cli "acao=procurar phy=$phy prioridade=ALTA volta=NOTIFICACAO repeticoes=1" "Coded $k, $(Nome $srv) anuncia"
                    if ($r.Desfecho -eq 'ok') { $okCoded++ } else { $codigos += "$($r.Desfecho) $($r.Codigo)".Trim() }
                }
                $marca = if ($okCoded -eq 3) { 'OK   ' } else { 'AVISO' }
                $resultados.Add("$marca Coded: $(Nome $cli) procurando em $phy`: $okCoded de 3$(if ($codigos) { " (falhas: $($codigos -join '; '))" })")
            }
        }
        PararTodos @($a, $b)
    }

    if ($Modo -eq 'Ambos') {
        # Sem calibracao, a analise nao aceita as medidas (regra 5.3): uma, a 1 m, antes.
        Write-Host '.. calibracao a 1 m antes do AMBOS'
        PararTodos @($Seriais[0], $Seriais[1])
        Anunciar $Seriais[0] | Out-Null
        $r = Rodada $Seriais[1] 'acao=calibrar phy=1M prioridade=PADRAO volta=NOTIFICACAO' '1 m, antes do AMBOS'
        if ($r.Desfecho -ne 'calibracao_OK') { throw "a calibracao antes do AMBOS nao deu OK ($($r.Desfecho) $($r.Codigo))" }
        Write-Host '.. AMBOS: 3 com espera aleatoria, 3 com espera 0 e 4 com espera 0 e o OLA segurado'
        RodadasAmbos $Seriais[0] $Seriais[1] 3 3 4 2
        PararTodos @($Seriais[0], $Seriais[1])
    }

    if ($Modo -eq 'Alcance') {
        $a = $Seriais[0]; $b = $Seriais[1]
        if ($Distancias -notmatch '^\d{1,3}(,\d{1,3}){0,9}$') { throw "distancias fora do formato: $Distancias" }
        PararTodos @($a, $b)
        Anunciar $a | Out-Null
        if (-not $SemCalibrar) {
            $r = Rodada $b 'acao=calibrar phy=1M prioridade=PADRAO volta=NOTIFICACAO' '1 m, antes do alcance'
            if ($r.Desfecho -ne 'calibracao_OK') { throw "a calibracao antes do alcance nao deu OK ($($r.Desfecho) $($r.Codigo)): nao adianta medir" }
        }
        # 2 idas e voltas por ponto: a volta Coded no sentido lento leva minutos (estimo 130 a 160 s).
        $r = Comando $b "acao=roteiro distancias=$Distancias combos=$combosAlcance repeticoes=2" 15
        if ($r.Desfecho -ne 'ok' -or -not (EsperarLinha $b 'sonda-ble roteiro evento=carregado' $r.Desde 5)) { throw "o roteiro de alcance nao carregou no $(Nome $b) (desfecho=$($r.Desfecho))" }
        $resultados.Add("OK    alcance: $(Nome $a) anunciando; roteiro carregado no $(Nome $b) ($Distancias m; $combosAlcance; 2 repeticoes)")
    }
    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
    # Deixa os celulares parados, para a leitura das medidas nao pegar nada no meio.
    if ($Modo -ne 'Alcance') {
        foreach ($s in $Seriais) { if ($como.ContainsKey($s)) { try { Comando $s 'acao=parar' 15 | Out-Null } catch { Write-Host "   aviso: nao parei a sonda do $(Nome $s)" } } }
    }
} finally {
    foreach ($linha in $resultados) { Write-Host $linha }
}

# Linha FALHA (rodada sem cliente, dois clientes) tambem derruba o resultado: um
# 'RESULTADO: OK' depois de uma FALHA ja enganou uma vez (regra 4.3).
$falhas = @($resultados | Where-Object { $_ -like 'FALHA*' }).Count
if ($ok -and $falhas -eq 0) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host "RESULTADO: FALHOU ($falhas linha(s) FALHA)"
exit 1
