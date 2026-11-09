# ferramentas/instalar-no-celular.ps1
# Compila o app (debug, arm64) uma vez, instala no(s) celular(es) ligado(s) no cabo,
# abre, confere o logcat e tira uma captura da tela de cada um. Tambem informa tamanho
# e densidade da tela, que entram nos testes de layout.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Serial <serial>
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -Relay
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -Relay -TelaLivre
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -RelayPublico -TelaLivre
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -SondaBle
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -LerSonda
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -SondaBle -Conferir
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -SondaBle -Autonomo
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -SondaBle -Autonomo -Alcance
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -SondaBle -Ambos
#
# Sem parametro: exige um unico celular no cabo. Com mais de um, use -Todos ou -Serial.
#
# -Relay: o teste com os dois celulares. Sobe o relay de teste nesta maquina (so em
# 127.0.0.1, malote na memoria), monta o app com o endereco dele, liga a porta a cada
# celular pelo adb reverse e fica esperando FIM enquanto o teste e feito na mao.
# No fim desliga o relay, desfaz os reverses e mostra as linhas do app no logcat de cada
# celular (o app so grava codigos, nunca texto, chave ou codigo de contato: regra 8.2).
# Os celulares ficam no cabo o tempo todo.
#
# -RelayPublico: o teste com os celulares fora do cabo. Usa o relay
# que o ferramentas\relay-publico.ps1 pos na internet pelo Tailscale Funnel, rodando em
# outra janela: le o endereco https://...ts.net que ele gravou, confere o /saude e monta
# o app com esse endereco. Sem adb reverse: o celular vai ao relay pela internet. O cabo
# continua so para instalar, ler o logcat e capturar. A espera por FIM e as capturas sao
# as mesmas do -Relay. Diz se o Wi-Fi de cada celular esta ligado: os celulares nao tem
# Tailscale, entao no Wi-Fi de casa ou no 4G vao igual pela internet e pelo Funnel (no
# teste de 08/11/2026, mandar desligar o Wi-Fi deixou celular sem dados moveis sem
# internet nenhuma). A conferencia do /saude daqui passa pela tailnet deste PC: o caminho
# publico e conferido no navegador do celular. Nao combina com -Relay.
#
# -SondaBle: a sonda medida do modo Direto (documentacao/arquitetura/modo-direto.md,
# passo 1). Exige os dois celulares no cabo. Monta o app de debug com
# --dart-define=KRYPTOS_SONDA_BLE=true, confere no APK as tres permissoes de Bluetooth
# (BLUETOOTH_SCAN com neverForLocation) e nenhuma de localizacao, confere que o Bluetooth
# de cada celular esta ligado, zera o arquivo de medidas de cada um e espera FIM. Durante
# o teste os celulares podem sair do cabo (5 m, 10 m e mais): a sonda grava as medidas num
# arquivo da pasta privada do app. No FIM, espera os dois de volta no cabo, traz os
# arquivos pelo run-as (so app de debug) para Downloads, num zip de nome unico, e roda o
# ferramentas\analisar-sonda-ble.ps1. Nao combina com -Relay nem -RelayPublico.
#
# -SondaBle com -Conferir, -Autonomo ou -Alcance: sem tocar na tela. A permissao de
# Bluetooth e dada pelo adb (pm grant, conferida no dumpsys) e o ferramentas\sonda-roteiro.ps1
# conduz a sonda pelo cabo, por um arquivo de comando na pasta privada do app.
#   -Conferir: um celular basta (ou cada um, com -Todos). Confere as pecas que o teste de
#     dois celulares precisa: o comando chega, L2CAP abre escuta, anuncia, recusa comando
#     errado, para, roteiro, procura sem o outro, recusa com o app fora da frente. Traz as
#     medidas sem a analise (sem o outro celular nao ha calibracao).
#   -Autonomo: os dois no cabo, a 1 m: calibra e mede os tres caminhos de volta nos dois
#     sentidos, AMBOS 3 vezes e o ciclo de vida. Depois traz as medidas e analisa.
#   -Ambos: os dois no cabo, a 1 m: so o AMBOS, 5 vezes com a espera aleatoria e 5 com
#     espera 0 (colisao provocada), para conferir o desempate pelo OLA.
#   -Alcance: carrega o roteiro de distancias (o segundo celular procura, o primeiro
#     anuncia); voce tira os cabos, leva os celulares a cada distancia e toca PROXIMO
#     PONTO; no fim, cabos de volta e FIM. Sem -Autonomo, calibra a 1 m antes.
#
# Coleta (-SondaBle e -LerSonda): tudo vai para um zip em Downloads, kryptos-sonda-<carimbo>.zip:
#   - as medidas de cada celular (files/sonda-ble.txt, pelo run-as);
#   - o logcat de cada celular: em fluxo do abrir do app ate o fim, mais o que estiver no
#     buffer do celular no fim (pega o trecho sem cabo do -Alcance); so as linhas do
#     processo do app e do Bluetooth; enderecos Bluetooth trocados por apelidos (bt-1,
#     bt-2...) e nomes de aparelho trocados por <oculto> (regra 8.2 e modo-direto.md);
#   - a analise (analisar-sonda-ble.ps1) num arquivo;
#   - a transcricao desta janela, sem o cabecalho do PowerShell (usuario e maquina);
#   - bateria e temperatura de cada celular no comeco e no fim, nas linhas de resultado.
#
# -LerSonda: so a leitura do -SondaBle, sem compilar, instalar nem zerar nada. Para quando
# o -SondaBle parou depois do teste (celular que nao voltou ao cabo, janela fechada): as
# medidas continuam nos celulares ate o proximo -SondaBle.
#
# -TelaLivre: so para filmar e capturar no teste. Monta o app sem FLAG_SECURE (o Android
# so aceita em app de debug) e com uma faixa laranja "TELA LIBERADA" em todas as telas.
# A captura passa a ter de mostrar conteudo.
#
# -Relay com -TelaLivre: durante o teste manual, captura as telas dos dois celulares
# sozinho, a cada linha nova do app no logcat (carta enviada, selada, nao abriu...) e a
# cada 15 s. As capturas vao para uma pasta em Downloads e, no fim, num zip de nome
# unico, pronto para anexar. Sem -TelaLivre a tela sai preta e nada e capturado.
#
# Captura: o app tem FLAG_SECURE, entao o centro da captura tem de sair preto. Isso e
# conferido (a barra de status e a de navegacao sao do sistema e aparecem). A prova de
# que a tela abriu passa a ser o processo vivo, o logcat limpo e a conferencia a olho.
# Saida: capturas em Downloads com nome unico (kryptos-tela-<modelo>-AAAAMMDD-HHMMSS.png).

[CmdletBinding()]
param(
    [switch]$Todos,
    [string]$Serial,
    [switch]$Relay,
    [switch]$RelayPublico,
    [switch]$TelaLivre,
    [switch]$SondaBle,
    [switch]$LerSonda,
    [switch]$Conferir,
    [switch]$Autonomo,
    [switch]$Ambos,
    [switch]$Alcance
)

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot
$arqAmb = Join-Path $PSScriptRoot 'ambiente.json'
if (-not (Test-Path -LiteralPath $arqAmb)) {
    Write-Host 'ERRO: falta ferramentas\ambiente.json (rode ferramentas\criar-estrutura.ps1)'
    exit 1
}
$Amb = Get-Content -LiteralPath $arqAmb -Raw -Encoding UTF8 | ConvertFrom-Json

$Pacote = 'com.torionsystems.kryptos'
$resultados = New-Object System.Collections.Generic.List[string]

# Porta do relay de teste, a mesma da suite. Estimo que esteja livre; se nao estiver,
# o relay para ao subir e o script diz.
$portaRelay = 18080
$procRelay = $null
$logsRelay = @()
$comReverse = New-Object System.Collections.Generic.List[string]
$jobCapturas = $null
$pararCapturas = $null
# Endereco que o relay-publico.ps1 grava enquanto esta no ar (o mesmo caminho dele).
# Medidas da sonda BLE: arquivo na pasta privada do app (SondaBle.kt), lido pelo run-as.
$medidasSonda = 'files/sonda-ble.txt'
$arquivosSonda = New-Object System.Collections.Generic.List[string]
$carimbo = Get-Date -Format 'yyyyMMdd-HHmmss'
$automatico = $Conferir -or $Autonomo -or $Ambos -or $Alcance
# Coleta: arquivos que vao para o zip no fim; logcat em fluxo por celular; transcricao.
$coleta = New-Object System.Collections.Generic.List[string]
$capturasLogcat = @{}
$apelidosBt = @{}
$transcricao = $null
$arqEnderecoPublico = Join-Path (Join-Path (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Kryptos') 'relay-publico') 'endereco.txt'
$enderecoPublico = $null

function Rodar([string]$Exe, [string[]]$Argumentos) {
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & $Exe @Argumentos 2>&1 } finally { $ErrorActionPreference = $antes }
    $codigo = $LASTEXITCODE
    $texto = (@($saida | ForEach-Object { "$_" }) -join "`n").Trim()
    if ($codigo -ne 0) {
        $fim = (@($texto -split "`n") | Select-Object -Last 30) -join "`n"
        throw "$([System.IO.Path]::GetFileName($Exe)) $($Argumentos -join ' ') saiu com codigo $codigo.`n$fim"
    }
    return $texto
}

function Passo([string]$Nome, [scriptblock]$Acao) {
    Write-Host ".. $Nome"
    $detalhe = & $Acao
    if ($detalhe) { $resultados.Add("OK    $Nome  ($detalhe)") } else { $resultados.Add("OK    $Nome") }
}

function Achar([string]$Pasta, [string[]]$Nomes) {
    # No Windows vale o .bat/.exe; fora dele (teste do Claude no Linux), o nome sem extensao.
    if ($env:OS -ne 'Windows_NT') { $Nomes = @($Nomes | ForEach-Object { [System.IO.Path]::GetFileNameWithoutExtension($_) }) }
    foreach ($n in $Nomes) {
        $c = Join-Path $Pasta $n
        if (Test-Path -LiteralPath $c) { return $c }
    }
    throw "nao achei $($Nomes -join ' / ') em $Pasta"
}

function Bateria([string]$s) {
    try {
        $t = Rodar $adb @('-s', $s, 'shell', 'dumpsys', 'battery')
        $nivel = if ($t -match 'level:\s*(\d+)') { "$($Matches[1])%" } else { '?' }
        $temp = if ($t -match 'temperature:\s*(\d+)') { [string]::Format([Globalization.CultureInfo]::InvariantCulture, '{0:0.0} C', [int]$Matches[1] / 10) } else { '?' }
        return "bateria $nivel, $temp"
    } catch { return "bateria nao lida ($(($_.Exception.Message -split "`n")[0]))" }
}

# Endereco Bluetooth vira apelido (o mesmo endereco, o mesmo apelido em todos os arquivos
# desta execucao); nome de aparelho vira <oculto>. O apelido nao sai desta execucao.
function Mascarar([string]$l) {
    $l = [regex]::Replace($l, '(?i)\b(?:[0-9a-fx]{2}[:-]){5}[0-9a-f]{2}\b', [System.Text.RegularExpressions.MatchEvaluator]{
        param($m)
        $k = $m.Value.ToUpperInvariant()
        if (-not $script:apelidosBt.ContainsKey($k)) { $script:apelidosBt[$k] = "bt-$($script:apelidosBt.Count + 1)" }
        $script:apelidosBt[$k]
    })
    return [regex]::Replace($l, '(?i)\b(name|nome|alias)\s*([=:])\s*("[^"]*"|''[^'']*''|[^,\s\]\}\)]+)', '$1$2<oculto>')
}

function IniciarLogcat([string]$s) {
    $base = Join-Path ([System.IO.Path]::GetTempPath()) ('kryptos-logcat-' + [guid]::NewGuid().ToString('N'))
    $p = Start-Process -FilePath $adb -ArgumentList @('-s', $s, 'logcat', '-v', 'threadtime', '-b', 'main,system,crash') `
        -RedirectStandardOutput "$base.txt" -RedirectStandardError "$base.err" -PassThru -NoNewWindow
    $script:capturasLogcat[$s] = [pscustomobject]@{ Proc = $p; Bruto = "$base.txt"; Erro = "$base.err"; Pids = (New-Object System.Collections.Generic.List[string]) }
}

function PararCapturasLogcat {
    foreach ($c in @($script:capturasLogcat.Values)) {
        try { if (-not $c.Proc.HasExited) { Stop-Process -Id $c.Proc.Id -Force } } catch { Write-Host "   aviso: nao parei o logcat ($($_.Exception.Message))" }
    }
}

function ApagarBrutosLogcat {
    foreach ($c in @($script:capturasLogcat.Values)) {
        foreach ($f in @($c.Bruto, $c.Erro)) { if (Test-Path -LiteralPath $f) { Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue } }
    }
}

# Fluxo + buffer do celular, so app e Bluetooth, mascarado, gravado e lido de volta.
function ColherLogcat($a, [string]$modeloArq) {
    $s = $a.Serial
    $c = $script:capturasLogcat[$s]
    if ($c) { try { if (-not $c.Proc.HasExited) { Stop-Process -Id $c.Proc.Id -Force; $c.Proc.WaitForExit(5000) | Out-Null } } catch { Write-Host "   aviso: logcat do $($a.Modelo): $($_.Exception.Message)" } }
    $linhas = New-Object System.Collections.Generic.List[string]
    $vistas = New-Object 'System.Collections.Generic.HashSet[string]'
    $doFluxo = 0
    if ($c -and (Test-Path -LiteralPath $c.Bruto)) {
        foreach ($l in [System.IO.File]::ReadLines($c.Bruto)) { if ($vistas.Add($l)) { $linhas.Add($l); $doFluxo++ } }
    }
    $doBuffer = 0
    foreach ($l in @((Rodar $adb @('-s', $s, 'logcat', '-d', '-v', 'threadtime', '-b', 'main,system,crash')) -split "`n")) {
        $l = $l.TrimEnd("`r")
        if ($vistas.Add($l)) { $linhas.Add($l); $doBuffer++ }
    }
    # Processos que interessam: o app (pid ao abrir e agora) e o servico de Bluetooth.
    $pids = New-Object 'System.Collections.Generic.HashSet[string]'
    if ($c) { foreach ($p in $c.Pids) { [void]$pids.Add($p) } }
    foreach ($proc in @($Pacote, 'com.android.bluetooth', 'com.google.android.bluetooth')) {
        try { foreach ($p in @((Rodar $adb @('-s', $s, 'shell', 'pidof', $proc)) -split '\s+')) { if ($p -match '^\d+$') { [void]$pids.Add($p) } } } catch { $null = $_ }
    }
    $saida = New-Object System.Collections.Generic.List[string]
    foreach ($l in $linhas) {
        if ($l -match '^\d\d-\d\d \d\d:\d\d:\d\d\.\d{3}\s+(\d+)\s+\d+\s+[VDIWEFA]\s' -and ($pids.Contains($Matches[1]) -or $l -match 'KryptosSonda')) {
            $saida.Add((Mascarar $l))
        }
    }
    $destino = Join-Path $Amb.downloads "kryptos-logcat-$modeloArq-$carimbo.txt"
    [System.IO.File]::WriteAllLines($destino, $saida.ToArray(), (New-Object System.Text.UTF8Encoding($false)))
    $lido = @([System.IO.File]::ReadAllLines($destino))
    if ($lido.Count -ne $saida.Count) { throw "o logcat gravado em $destino nao bate com o colhido ($($lido.Count) de $($saida.Count) linhas)" }
    $vaza = @($lido | Where-Object { $_ -match '(?i)\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b' })
    if ($vaza.Count -gt 0) {
        Remove-Item -LiteralPath $destino -Force
        if (Test-Path -LiteralPath $destino) { throw "o logcat de $destino ficou com endereco Bluetooth e nao consegui apagar: apague a mao" }
        throw "o logcat mascarado ainda tinha $($vaza.Count) linhas com endereco Bluetooth: apagado, nao vai para o zip"
    }
    $script:coleta.Add($destino)
    return "$destino, $($saida.Count) linhas do app e do Bluetooth (fluxo $doFluxo, so no buffer $doBuffer; processos $(@($pids).Count)); $($script:apelidosBt.Count) enderecos mascarados"
}

# Tira o cabecalho e o rodape que o PowerShell poe na transcricao (usuario, maquina,
# caminho do executavel), grava e le de volta.
function LimparTranscricao([string]$arq) {
    $l = @([System.IO.File]::ReadAllLines($arq))
    $estrelas = @(for ($i = 0; $i -lt $l.Count; $i++) { if ($l[$i] -match '^\*{10,}$') { $i } })
    $fim = $l.Count
    $ini = 0
    if ($estrelas.Count -ge 2) { $ini = $estrelas[1] + 1 }
    if ($estrelas.Count -ge 4) { $fim = $estrelas[$estrelas.Count - 2] }
    if ($fim -le $ini) { $corpo = @() } else { $corpo = @($l[$ini..($fim - 1)] | Where-Object { $_ -notmatch '^(Transcript started|Transcricao iniciada|Transcri.+iniciada)' }) }
    [System.IO.File]::WriteAllLines($arq, $corpo, (New-Object System.Text.UTF8Encoding($false)))
    if (@([System.IO.File]::ReadAllLines($arq)) -match '^(Username|RunAs User|Machine|Host Application):') { throw 'a transcricao ainda tem o cabecalho' }
}

# Traz o arquivo de medidas da sonda de cada celular (run-as, so app de debug), grava em
# Downloads, le de volta, zipa e roda o analisar-sonda-ble.ps1. Usada pelo -SondaBle depois
# do FIM e pelo -LerSonda.
function LerEAnalisarSonda([switch]$SemAnalise) {
    Passo 'celulares de volta no cabo' {
        $limite = (Get-Date).AddSeconds(180)
        do {
            $ligadosAgora = @((Rodar $adb @('devices')) -split "`n" | Select-Object -Skip 1 |
                ForEach-Object { $p = $_.Trim() -split '\s+'; if ($p.Count -ge 2 -and $p[1] -eq 'device') { $p[0] } })
            $faltam = @($aparelhos | Where-Object { $ligadosAgora -notcontains $_.Serial })
            if ($faltam.Count -eq 0) { break }
            Write-Host "   esperando no cabo: $((@($faltam | ForEach-Object { $_.Modelo })) -join ', ')"
            Start-Sleep -Seconds 3
        } while ((Get-Date) -lt $limite)
        if ($faltam.Count -gt 0) { throw "em 180 s nao voltaram ao cabo: $((@($faltam | ForEach-Object { $_.Modelo })) -join ', '). As medidas continuam nos celulares: com os dois no cabo, rode o mesmo script com -Todos -LerSonda" }
        "$($aparelhos.Count) de $($aparelhos.Count)"
    }
    foreach ($a in $aparelhos) {
        $modeloArq = ($a.Modelo.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')
        Passo "$($a.Modelo) : medidas trazidas" {
            # exec-out: os bytes como estao, sem o terminal do adb no meio.
            $t = Rodar $adb @('-s', $a.Serial, 'exec-out', 'run-as', $Pacote, 'cat', $medidasSonda)
            if (-not $t) { throw "o arquivo de medidas do $($a.Modelo) esta vazio: a sonda nao chegou a medir" }
            $destino = Join-Path $Amb.downloads "kryptos-sonda-$modeloArq-$carimbo.txt"
            [System.IO.File]::WriteAllText($destino, $t + "`n", (New-Object System.Text.UTF8Encoding($false)))
            $lido = [System.IO.File]::ReadAllText($destino)
            if ($lido.TrimEnd() -ne $t.TrimEnd()) { throw "o arquivo gravado em $destino nao bate com o lido do celular" }
            $arquivosSonda.Add($destino)
            $coleta.Add($destino)
            "$destino, $(@($t -split "`n").Count) linhas"
        }
        Passo "$($a.Modelo) : logcat (app e Bluetooth, enderecos mascarados)" { ColherLogcat $a $modeloArq }
        Passo "$($a.Modelo) : estado no fim" { Bateria $a.Serial }
    }
    if ($SemAnalise) { return }
    $ps = (Get-Process -Id $PID).Path
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $saidaAnalise = @(& $ps -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'analisar-sonda-ble.ps1') -Arquivos ($arquivosSonda -join ',') 2>&1)
    } finally { $ErrorActionPreference = $antes }
    $codigoAnalise = $LASTEXITCODE
    $resultados.Add('..    analise da sonda (ferramentas\analisar-sonda-ble.ps1):')
    foreach ($l in $saidaAnalise) { $resultados.Add("      $l") }
    $arqAnalise = Join-Path $Amb.downloads "kryptos-analise-$carimbo.txt"
    [System.IO.File]::WriteAllLines($arqAnalise, [string[]]@($saidaAnalise | ForEach-Object { "$_" }), (New-Object System.Text.UTF8Encoding($false)))
    $coleta.Add($arqAnalise)
    if ($codigoAnalise -ne 0) { throw 'a analise da sonda saiu com FALHA (ver acima)' }
}

$ok = $false
try {
    if ($SondaBle -or $LerSonda) {
        $transcricao = Join-Path $Amb.downloads "kryptos-execucao-$carimbo.txt"
        try { Start-Transcript -LiteralPath $transcricao -Force | Out-Null }
        catch { Write-Host "   aviso: sem transcricao ($($_.Exception.Message))"; $transcricao = $null }
    }
    if ($Todos -and $Serial) { throw 'use -Todos ou -Serial, nao os dois' }
    if ($Relay -and $RelayPublico) { throw 'use -Relay ou -RelayPublico, nao os dois' }
    if (($SondaBle -or $LerSonda) -and ($Relay -or $RelayPublico)) { throw 'a sonda BLE nao usa relay: -SondaBle e -LerSonda nao combinam com -Relay nem -RelayPublico' }
    if ($SondaBle -and $LerSonda) { throw 'use -SondaBle (teste inteiro) ou -LerSonda (so a leitura), nao os dois' }
    if ($automatico -and -not $SondaBle) { throw '-Conferir, -Autonomo, -Ambos e -Alcance sao modos da sonda: use junto com -SondaBle' }
    if ($Conferir -and ($Autonomo -or $Ambos -or $Alcance)) { throw '-Conferir e a conferencia com um celular: nao combina com -Autonomo, -Ambos nem -Alcance' }
    $flutter = Achar (Join-Path $Amb.flutter 'bin') @('flutter.bat', 'flutter')
    $adb = Achar (Join-Path $Amb.android_sdk 'platform-tools') @('adb.exe', 'adb')
    $app = Join-Path (Join-Path $Repo 'frontend') 'app'

    # ---------- quais celulares ----------
    $aparelhos = @()
    Passo 'celulares no adb' {
        $series = @((Rodar $adb @('devices')) -split "`n" | Select-Object -Skip 1 |
            ForEach-Object { $p = $_.Trim() -split '\s+'; if ($p.Count -ge 2 -and $p[1] -eq 'device') { $p[0] } })
        if ($series.Count -eq 0) { throw 'nenhum celular autorizado no adb' }
        $lista = @(foreach ($s in $series) {
            [pscustomobject]@{
                Serial  = $s
                Modelo  = Rodar $adb @('-s', $s, 'shell', 'getprop', 'ro.product.model')
                Android = Rodar $adb @('-s', $s, 'shell', 'getprop', 'ro.build.version.release')
            }
        })
        $descricao = (@($lista | ForEach-Object { "$($_.Modelo) [$($_.Serial)]" }) -join ', ')
        if ($Serial) {
            $escolhidos = @($lista | Where-Object { $_.Serial -eq $Serial })
            if ($escolhidos.Count -eq 0) { throw "serial $Serial nao esta no adb; ligados: $descricao" }
        } elseif ($Todos) {
            $escolhidos = @($lista)
        } else {
            if ($lista.Count -ne 1) { throw "ha $($lista.Count) celulares ligados ($descricao). Use -Todos ou -Serial <serial>" }
            $escolhidos = @($lista)
        }
        if (($SondaBle -or $LerSonda) -and -not $Conferir -and $escolhidos.Count -ne 2) { throw "a sonda BLE precisa de dois celulares, um de cada papel; escolhidos: $($escolhidos.Count). Ligue os dois no cabo e use -Todos (com um so: -SondaBle -Conferir)" }
        $script:aparelhos = $escolhidos
        (@($escolhidos | ForEach-Object { "$($_.Modelo), Android $($_.Android)" }) -join ' | ')
    }

    if ($LerSonda) {
        LerEAnalisarSonda
    } else {
    # (o resto do script fica dentro deste else sem reindentar, para o diff ficar legivel)

    # ---------- Rust: o Cargokit compila com a stable, que tem de ser a do rust-toolchain.toml ----------
    Passo 'versao do Rust' {
        $ps = (Get-Process -Id $PID).Path
        $t = Rodar $ps @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $PSScriptRoot 'conferir-rust.ps1'), '-Raiz', $Repo)
        if ($t -match 'OK\s+versao do Rust: (\S+)') { "$($Matches[1]) na suite e no celular" }
    }

    # ---------- relay de teste (so com -Relay) ----------
    if ($Relay) {
        Passo 'relay de teste nesta maquina' {
            Set-Location -LiteralPath $Repo
            Rodar 'cargo' @('build', '-p', 'kryptos_relay') | Out-Null
            $exeRelay = Achar (Join-Path (Join-Path $Repo 'target') 'debug') @('kryptos_relay.exe', 'kryptos_relay')
            $base = Join-Path ([System.IO.Path]::GetTempPath()) ('kryptos-relay-' + [guid]::NewGuid().ToString('N'))
            $script:logsRelay = @("$base.log", "$base.err")
            $env:KRYPTOS_RELAY_ENDERECO = "127.0.0.1:$portaRelay"
            try {
                $script:procRelay = Start-Process -FilePath $exeRelay -PassThru -NoNewWindow `
                    -RedirectStandardOutput $script:logsRelay[0] -RedirectStandardError $script:logsRelay[1]
            } finally {
                Remove-Item Env:\KRYPTOS_RELAY_ENDERECO -ErrorAction SilentlyContinue
            }
            # Espera o /saude ate 10 s. Cada tentativa que falha so quer dizer "ainda nao
            # subiu"; se nao subir, a falha sai abaixo com o motivo.
            $pronto = $false
            for ($i = 0; $i -lt 50 -and -not $pronto; $i++) {
                if ($script:procRelay.HasExited) { throw "o relay de teste parou ao subir (porta $portaRelay ocupada?)" }
                try {
                    $r = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$portaRelay/saude" -TimeoutSec 2
                    if ($r.StatusCode -eq 200) { $pronto = $true }
                } catch {
                    Start-Sleep -Milliseconds 200
                }
            }
            if (-not $pronto) { throw "o relay de teste nao respondeu em 10 s na porta $portaRelay" }
            "127.0.0.1:$portaRelay, malote na memoria"
        }
    }

    # ---------- relay na internet (so com -RelayPublico) ----------
    if ($RelayPublico) {
        Passo 'relay na internet (relay-publico.ps1)' {
            if (-not (Test-Path -LiteralPath $arqEnderecoPublico)) {
                throw "nao achei $arqEnderecoPublico. Rode antes, em outra janela: powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\relay-publico.ps1"
            }
            $e = ([System.IO.File]::ReadAllText($arqEnderecoPublico)).Trim()
            if ($e -notmatch '^https://[a-z0-9-]+\.[a-z0-9-]+\.ts\.net$') { throw "endereco inesperado em $arqEnderecoPublico : '$e'" }
            # curl.exe: no Windows PowerShell 5.1, "curl" e apelido do Invoke-WebRequest.
            $curl = if ($env:OS -eq 'Windows_NT') { 'curl.exe' } else { 'curl' }
            $antes = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'
            try { $cod = (@(& $curl -sS -o ([System.IO.Path]::GetTempPath() + 'kryptos-saude.txt') -w '%{http_code}' --max-time 20 "$e/saude" 2>&1) | ForEach-Object { "$_" }) -join ' ' }
            finally { $ErrorActionPreference = $antes; Remove-Item -LiteralPath ([System.IO.Path]::GetTempPath() + 'kryptos-saude.txt') -Force -ErrorAction SilentlyContinue }
            if ($cod.Trim() -notmatch '200$') { throw "o relay em $e nao respondeu (/saude: $($cod.Trim())). O relay-publico.ps1 ainda esta aberto na outra janela?" }
            $script:enderecoPublico = $e
            "$e, /saude 200 a partir deste PC (pela tailnet; o caminho publico se confere no celular)"
        }
    }

    # ---------- compilar uma vez ----------
    $apk = Join-Path $app 'build\app\outputs\flutter-apk\app-debug.apk'
    Passo 'compilar APK de debug (arm64)' {
        Set-Location -LiteralPath $app
        Write-Host '   Gradle + Rust para Android; a primeira vez demora'
        $argsBuild = @('build', 'apk', '--debug', '--target-platform', 'android-arm64')
        if ($Relay) { $argsBuild += "--dart-define=KRYPTOS_RELAY=http://127.0.0.1:$portaRelay" }
        if ($RelayPublico) { $argsBuild += "--dart-define=KRYPTOS_RELAY=$enderecoPublico" }
        if ($TelaLivre) { $argsBuild += '--dart-define=KRYPTOS_TELA_LIVRE=true' }
        if ($SondaBle) { $argsBuild += '--dart-define=KRYPTOS_SONDA_BLE=true' }
        Rodar $flutter $argsBuild | Out-Null
        if (-not (Test-Path -LiteralPath $apk)) { throw "o build terminou mas nao achei $apk" }
        $comRelay = if ($Relay) { 'com o relay de teste' } elseif ($RelayPublico) { "com o relay $enderecoPublico" } else { 'sem relay' }
        $comTela = if ($TelaLivre) { 'TELA LIVRE (sem FLAG_SECURE)' } else { 'tela segura' }
        $comSonda = if ($SondaBle) { ', SONDA BLE no lugar do app' } else { '' }
        "$([math]::Round((Get-Item -LiteralPath $apk).Length / 1MB, 1)) MB, $comRelay, $comTela$comSonda"
    }

    if ($SondaBle) {
        Passo 'permissoes da sonda no APK' {
            # O manifesto final, de dentro do APK: o que o Android vai ver.
            $aapt = Achar (Join-Path (Join-Path $Amb.android_sdk 'build-tools') '36.0.0') @('aapt2.exe', 'aapt2')
            $xml = Rodar $aapt @('dump', 'xmltree', '--file', 'AndroidManifest.xml', $apk)
            $blocos = @($xml -split 'E: uses-permission' | Select-Object -Skip 1)
            $nomes = @($blocos | ForEach-Object { if ($_ -match 'android:name\(0x[0-9a-f]+\)="([^"]+)"') { $Matches[1] } })
            $faltando = @('BLUETOOTH_SCAN', 'BLUETOOTH_ADVERTISE', 'BLUETOOTH_CONNECT' | Where-Object { $nomes -notcontains "android.permission.$_" })
            if ($faltando.Count -gt 0) { throw "o APK nao pede $($faltando -join ', ')" }
            $proibidas = @('ACCESS_FINE_LOCATION', 'ACCESS_COARSE_LOCATION', 'ACCESS_BACKGROUND_LOCATION', 'BLUETOOTH', 'BLUETOOTH_ADMIN' |
                Where-Object { $nomes -contains "android.permission.$_" })
            if ($proibidas.Count -gt 0) { throw "o APK pede $($proibidas -join ', '): o modo Direto nao usa localizacao nem as permissoes antigas" }
            $scan = @($blocos | Where-Object { $_ -match '"android\.permission\.BLUETOOTH_SCAN"' })[0]
            # neverForLocation = 0x00010000 em usesPermissionFlags.
            if ($scan -notmatch 'usesPermissionFlags\(0x[0-9a-f]+\)=0x([0-9a-f]+)' -or (([Convert]::ToInt32($Matches[1], 16)) -band 0x10000) -eq 0) {
                throw 'BLUETOOTH_SCAN sem neverForLocation no APK'
            }
            'BLUETOOTH_SCAN (neverForLocation), BLUETOOTH_ADVERTISE, BLUETOOTH_CONNECT; nenhuma de localizacao'
        }
    }

    # ---------- em cada celular ----------
    foreach ($a in $aparelhos) {
        # Nomes que nao colidem com os parametros de Passo ($Nome, $Acao): o bloco roda
        # dentro de Passo e o PowerShell nao diferencia maiusculas.
        $s = $a.Serial
        $modeloAtual = $a.Modelo

        Passo "$modeloAtual : tela" {
            $tam = Rodar $adb @('-s', $s, 'shell', 'wm', 'size')
            $den = Rodar $adb @('-s', $s, 'shell', 'wm', 'density')
            # Vale o "Override" se o usuario mudou o tamanho de exibicao; senao o "Physical".
            $px = if ($tam -match 'Override size: (\d+x\d+)') { $Matches[1] } elseif ($tam -match '(\d+x\d+)') { $Matches[1] } else { '?' }
            $dpi = if ($den -match 'Override density: (\d+)') { [int]$Matches[1] } elseif ($den -match '(\d+)') { [int]$Matches[1] } else { 0 }
            if ($dpi -gt 0) { "$px px, densidade $dpi dpi = $([math]::Round($dpi / 160, 3)) px por dp" } else { "$px px, densidade nao lida: $den" }
        }

        Passo "$modeloAtual : instalar" {
            Write-Host '   no HyperOS pode aparecer um aviso na tela do celular: permita'
            $t = Rodar $adb @('-s', $s, 'install', '-r', $apk)
            if ($t -notmatch 'Success') { throw "adb install nao disse Success: $t" }
            'Success'
        }

        Passo "$modeloAtual : permissoes" {
            # O plugin da camera declara microfone e escrita no armazenamento; o
            # AndroidManifest.xml do app tira os dois. Confere no app instalado.
            $pacoteInstalado = Rodar $adb @('-s', $s, 'shell', 'dumpsys', 'package', $Pacote)
            if ($pacoteInstalado -notmatch 'android\.permission\.CAMERA') { throw 'o app instalado nao pede a camera' }
            # Localizacao nunca (modo-direto.md): nem no app de debug.
            $sobrando = @('RECORD_AUDIO', 'WRITE_EXTERNAL_STORAGE', 'READ_EXTERNAL_STORAGE', 'FOREGROUND_SERVICE_CAMERA',
                'ACCESS_FINE_LOCATION', 'ACCESS_COARSE_LOCATION', 'ACCESS_BACKGROUND_LOCATION' |
                Where-Object { $pacoteInstalado -match "android\.permission\.$_\b" })
            if ($sobrando.Count -gt 0) { throw "o app instalado pede permissao que nao usa: $($sobrando -join ', ')" }
            if ($SondaBle) {
                $semBt = @('BLUETOOTH_SCAN', 'BLUETOOTH_ADVERTISE', 'BLUETOOTH_CONNECT' | Where-Object { $pacoteInstalado -notmatch "android\.permission\.$_\b" })
                if ($semBt.Count -gt 0) { throw "o app instalado nao pede $($semBt -join ', ')" }
                'camera e Bluetooth sim; microfone, armazenamento e localizacao nao'
            } else {
                'camera sim; microfone, armazenamento e localizacao nao'
            }
        }

        if ($Relay) {
            Passo "$modeloAtual : porta do relay" {
                Rodar $adb @('-s', $s, 'reverse', "tcp:$portaRelay", "tcp:$portaRelay") | Out-Null
                $comReverse.Add($s)
                "tcp:$portaRelay do celular -> tcp:$portaRelay desta maquina"
            }
        }

        if ($RelayPublico) {
            # So informa: sem Tailscale no celular, o caminho e pela internet tanto no
            # Wi-Fi quanto no 4G.
            Passo "$modeloAtual : rede" {
                $w = ''
                try { $w = ((Rodar $adb @('-s', $s, 'shell', 'cmd', 'wifi', 'status')) -split "`n")[0].Trim() }
                catch { $w = "nao consegui ler ($($_.Exception.Message -split "`n" | Select-Object -First 1))" }
                if ($w -match 'disabled') { 'Wi-Fi desligado: so os dados moveis (confira que funcionam)' }
                elseif ($w -match 'enabled') { 'Wi-Fi ligado: vai pela internet e pelo Funnel' }
                else { "Wi-Fi: $w" }
            }
        }

        if ($SondaBle) {
            Passo "$modeloAtual : Bluetooth" {
                $bt = (Rodar $adb @('-s', $s, 'shell', 'settings', 'get', 'global', 'bluetooth_on')).Trim()
                if ($bt -ne '1') { throw "o Bluetooth do $modeloAtual esta desligado (bluetooth_on=$bt): ligue e rode de novo" }
                $w = ''
                try { $w = ((Rodar $adb @('-s', $s, 'shell', 'cmd', 'wifi', 'status')) -split "`n")[0].Trim() }
                catch { $w = "nao lido ($($_.Exception.Message -split "`n" | Select-Object -First 1))" }
                # O Wi-Fi divide os 2,4 GHz com o Bluetooth: so registra, para comparar rodadas.
                $estadoWifi = if ($w -match 'disabled') { 'desligado' } elseif ($w -match 'enabled') { 'ligado' } else { $w }
                "ligado; Wi-Fi $estadoWifi (mantenha igual em todas as rodadas); $(Bateria $s)"
            }
            if ($automatico) {
                # Sem tocar na tela: a permissao "dispositivos por perto" pelo adb, e
                # conferida no app instalado (o que grava e provado lendo de volta).
                Passo "$modeloAtual : Bluetooth permitido pelo adb (pm grant)" {
                    foreach ($p in @('BLUETOOTH_SCAN', 'BLUETOOTH_ADVERTISE', 'BLUETOOTH_CONNECT')) {
                        Rodar $adb @('-s', $s, 'shell', 'pm', 'grant', $Pacote, "android.permission.$p") | Out-Null
                    }
                    $dump = Rodar $adb @('-s', $s, 'shell', 'dumpsys', 'package', $Pacote)
                    $semGrant = @('BLUETOOTH_SCAN', 'BLUETOOTH_ADVERTISE', 'BLUETOOTH_CONNECT' | Where-Object { $dump -notmatch "android\.permission\.$_`: granted=true" })
                    if ($semGrant.Count -gt 0) { throw "depois do pm grant, o dumpsys ainda nao mostra concedida: $($semGrant -join ', ')" }
                    'as tres concedidas e conferidas no dumpsys; nenhuma de localizacao'
                }
            }
            Passo "$modeloAtual : medidas da sonda zeradas" {
                Rodar $adb @('-s', $s, 'shell', 'run-as', $Pacote, 'rm', '-f', $medidasSonda) | Out-Null
                # Sem sh -c: o adb junta os argumentos numa linha e o shell do celular le de
                # novo, e o ';' de um comando composto quebrava (teste de 08/11/2026). Um ls
                # de um arquivo so, sem nada para o shell interpretar: "No such file" = apagado.
                $antes = $ErrorActionPreference
                $ErrorActionPreference = 'Continue'
                try { $ls = (@(& $adb -s $s shell run-as $Pacote ls $medidasSonda 2>&1) | ForEach-Object { "$_" }) -join ' ' }
                finally { $ErrorActionPreference = $antes }
                if ($ls -match 'No such file') { "$medidasSonda apagado e conferido" }
                elseif ($ls -match [regex]::Escape($medidasSonda)) { throw "o arquivo de medidas continuou no $modeloAtual" }
                else { throw "nao consegui conferir o arquivo de medidas no $modeloAtual (run-as): $ls" }
            }
        }

        Passo "$modeloAtual : abrir o app" {
            try { Rodar $adb @('-s', $s, 'logcat', '-c') | Out-Null } catch { Write-Host "   aviso: nao limpei o logcat; a conferencia pode pegar erro antigo" }
            # Sonda: o logcat inteiro desta execucao, em fluxo, ate a leitura no fim.
            if ($SondaBle) { IniciarLogcat $s }
            Rodar $adb @('-s', $s, 'shell', 'am', 'start', '-W', '-n', "$Pacote/.MainActivity") | Out-Null
            Start-Sleep -Seconds 5
            $processo = $null
            try { $processo = Rodar $adb @('-s', $s, 'shell', 'pidof', $Pacote) } catch { $processo = $null }
            if (-not $processo) {
                $log = Rodar $adb @('-s', $s, 'logcat', '-d', '-t', '200', '*:E')
                throw "o app nao ficou aberto. Ultimos erros do logcat:`n$log"
            }
            if ($capturasLogcat.ContainsKey($s)) { foreach ($p in @("$processo" -split '\s+')) { if ($p -match '^\d+$') { $capturasLogcat[$s].Pids.Add($p) } } }
            "processo $processo"
        }

        Passo "$modeloAtual : sem erro fatal no logcat" {
            $log = Rodar $adb @('-s', $s, 'logcat', '-d', '-t', '2000')
            $ruins = @($log -split "`n" | Where-Object { $_ -match 'FATAL EXCEPTION|UnsatisfiedLinkError|PanicException|E/flutter|\[ERROR:flutter' })
            if ($ruins.Count -gt 0) { throw "logcat com erro:`n$($ruins | Select-Object -First 15 | Out-String)" }
            'nenhum'
        }

        Passo "$modeloAtual : captura da tela" {
            # -SondaBle: na primeira abertura a sonda pede "dispositivos por perto", e o aviso
            # e uma janela do Android, fora do FLAG_SECURE: a captura pega o aviso (teste de
            # 08/11/2026: captura as 14:06:04, permissao dada as 14:06:05). Sem a permissao,
            # nada se conclui pela captura; com ela, a conferencia roda como sempre.
            if ($SondaBle) {
                $perm = Rodar $adb @('-s', $s, 'shell', 'dumpsys', 'package', $Pacote)
                if ($perm -notmatch 'android\.permission\.BLUETOOTH_SCAN: granted=true') {
                    return 'SEM CONFERENCIA: o aviso de permissao do Android esta na tela (janela do sistema, fora do FLAG_SECURE). Toque Permitir no celular; a tela segura e conferida quando a permissao ja foi dada'
                }
            }
            # System.Drawing so existe no Windows: fora dele (calibracao na maquina do
            # Claude, com adb falso) a captura fica sem conferencia, e o resultado diz.
            if ($env:OS -ne 'Windows_NT') { return 'SEM CONFERENCIA: a captura so e conferida no Windows (System.Drawing)' }
            $modeloArq = ($modeloAtual.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')
            $destino = Join-Path $Amb.downloads ("kryptos-tela-$modeloArq-" + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.png')
            $remoto = '/sdcard/kryptos-tela.png'
            Rodar $adb @('-s', $s, 'shell', 'screencap', '-p', $remoto) | Out-Null
            try { Rodar $adb @('-s', $s, 'pull', $remoto, $destino) | Out-Null }
            finally { try { Rodar $adb @('-s', $s, 'shell', 'rm', '-f', $remoto) | Out-Null } catch { Write-Host "   aviso: nao apaguei $remoto do celular: $($_.Exception.Message)" } }
            $b = [System.IO.File]::ReadAllBytes($destino)
            if ($b.Length -lt 8 -or $b[0] -ne 0x89 -or $b[1] -ne 0x50 -or $b[2] -ne 0x4E -or $b[3] -ne 0x47) { throw "$destino nao e um PNG valido" }
            # FLAG_SECURE: o centro (de 20% a 80% da altura, fora das barras do sistema)
            # sai preto. Amostra de 40 x 40 pontos; "preto" = os tres canais ate 8.
            Add-Type -AssemblyName System.Drawing
            $img = [System.Drawing.Bitmap]::FromFile($destino)
            try {
                $acesos = 0; $total = 0
                for ($iy = 0; $iy -lt 40; $iy++) {
                    $y = [int]($img.Height * (0.2 + 0.6 * $iy / 39))
                    for ($ix = 0; $ix -lt 40; $ix++) {
                        $x = [int](($img.Width - 1) * $ix / 39)
                        $c = $img.GetPixel($x, $y)
                        $total++
                        if ($c.R -gt 8 -or $c.G -gt 8 -or $c.B -gt 8) { $acesos++ }
                    }
                }
            } finally { $img.Dispose() }
            if ($TelaLivre) {
                if ($acesos -eq 0) { throw "captura preta com -TelaLivre: a tela nao foi liberada ($destino)" }
                "$destino, $([math]::Round($b.Length / 1KB)) KB; centro com conteudo ($acesos de $total pontos): tela liberada para o teste"
            } else {
                if ($acesos -gt 0) { throw "$acesos de $total pontos do centro da captura tem conteudo: o FLAG_SECURE nao valeu ($destino)" }
                "$destino, $([math]::Round($b.Length / 1KB)) KB; centro preto ($total de $total pontos): FLAG_SECURE valendo"
            }
        }
    }

    if ($SondaBle -and $automatico) {
        $ps = (Get-Process -Id $PID).Path
        $roteiro = Join-Path $PSScriptRoot 'sonda-roteiro.ps1'
        $seriaisArg = (@($aparelhos | ForEach-Object { $_.Serial })) -join ','
        $modelosArg = (@($aparelhos | ForEach-Object { $_.Modelo })) -join ','
        $roteiroOk = $true
        $etapas = @()
        if ($Conferir) { $etapas += , @('Conferir') }
        if ($Autonomo) { $etapas += , @('UmMetro') }
        if ($Ambos) { $etapas += , @('Ambos') }
        if ($Alcance) { $etapas += , @(@('Alcance') + $(if ($Autonomo) { @('-SemCalibrar') } else { @() })) }
        foreach ($etapa in $etapas) {
            $resultados.Add("..    sonda-roteiro.ps1 -Modo $($etapa[0]):")
            Write-Host ".. sonda-roteiro.ps1 -Modo $($etapa[0]) (as linhas abaixo sao o andamento)"
            $antes = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'
            try {
                $argsRoteiro = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $roteiro, '-Modo', $etapa[0], '-Adb', $adb, '-Seriais', $seriaisArg, '-Modelos', $modelosArg) + @($etapa | Select-Object -Skip 1)
                & $ps @argsRoteiro 2>&1 | ForEach-Object {
                    $l = "$_"
                    Write-Host "   $l"
                    if ($l -match '^(OK|AVISO|FALHA|RESULTADO)') { $resultados.Add("      $l") }
                }
            } finally { $ErrorActionPreference = $antes }
            if ($LASTEXITCODE -ne 0) { $roteiroOk = $false; break }
        }
        if ($roteiroOk -and $Alcance) {
            Write-Host ''
            Write-Host 'Roteiro de alcance carregado. Agora:'
            Write-Host '  1. Tire os dois cabos. Nos dois celulares, Nao perturbe ligado e a sonda aberta na frente.'
            Write-Host '  2. Trena, linha reta, sem parede. Leve os celulares a distancia que o celular que procura mostra.'
            Write-Host '  3. Toque PROXIMO PONTO no celular que procura e nao mexa ate aparecer o proximo ponto.'
            Write-Host '  4. Quando aparecer roteiro-fim (ou nada mais achar), ligue os dois no cabo e digite FIM aqui.'
            try { $Host.UI.RawUI.FlushInputBuffer() } catch { Write-Host "   aviso: nao consegui limpar o buffer do teclado; so FIM encerra" }
            $inicioDaEspera = Get-Date
            do { $resposta = Read-Host 'Digite FIM para encerrar' } while ($resposta.Trim().ToUpperInvariant() -ne 'FIM')
            $resultados.Add("OK    espera do alcance: $([math]::Round(((Get-Date) - $inicioDaEspera).TotalMinutes, 1)) min")
        }
        # As medidas vem mesmo quando o roteiro falhou: sao o rastro do que aconteceu.
        LerEAnalisarSonda -SemAnalise:$Conferir
        if (-not $roteiroOk) { throw 'o roteiro da sonda falhou (ver acima); as medidas foram trazidas para o rastro' }
    }

    if ($Relay -or $RelayPublico -or ($SondaBle -and -not $automatico)) {
        Write-Host ''
        if ($SondaBle) {
            Write-Host 'Sonda BLE aberta nos dois celulares. Faca o roteiro de'
            Write-Host 'documentacao/arquitetura/modo-direto.md, secao "Teste da sonda", com o Nao perturbe'
            Write-Host 'ligado. Os celulares podem sair do cabo durante o teste.'
            Write-Host 'Digite FIM aqui so no fim do roteiro, com os dois celulares de volta no cabo.'
        } elseif ($Relay) {
            Write-Host 'Relay de teste no ar. Faca o roteiro do teste com os dois celulares'
            Write-Host '(documentacao/arquitetura/telas-de-conversa.md, secao "Teste com os dois celulares")'
            Write-Host 'e digite FIM aqui quando terminar. O relay desliga e os reverses saem.'
        } else {
            Write-Host "Apps montados com o relay $enderecoPublico. Faca o roteiro do teste fora do cabo"
            Write-Host '(documentacao/arquitetura/relay-publico.md) e digite FIM aqui quando terminar.'
            Write-Host "Antes, no navegador de cada celular: $enderecoPublico/saude (tem de aparecer: ok)"
            Write-Host 'O relay continua no ar na outra janela ate o FIM de la.'
        }
        # Teclas que sobraram do bloco colado no console (um Enter a mais) encerravam a
        # espera na hora, antes do teste. Limpa o que estiver no buffer e so aceita FIM.
        try { $Host.UI.RawUI.FlushInputBuffer() } catch { Write-Host "   aviso: nao consegui limpar o buffer do teclado ($($_.Exception.GetType().Name)); so FIM encerra" }
        $pastaCapturas = $null
        if ($TelaLivre) {
            $pastaCapturas = Join-Path $Amb.downloads ('kryptos-teste-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
            New-Item -ItemType Directory -Path $pastaCapturas | Out-Null
            $script:pararCapturas = Join-Path $pastaCapturas 'parar.sinal'
            $alvos = @($aparelhos | ForEach-Object {
                [pscustomobject]@{ Serial = $_.Serial; Nome = ($_.Modelo.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-') }
            })
            $script:jobCapturas = Start-Job -ArgumentList $adb, $alvos, $pastaCapturas, $script:pararCapturas -ScriptBlock {
                param($adb, $alvos, $pasta, $parar)
                $registro = Join-Path $pasta 'eventos.txt'
                # Ultima linha do app ja vista em cada celular. Pelo conteudo, nao pela
                # contagem: quando o app reabre, o logcat do celular pode recomecar
                # (visto em 08/11/2026) e a contagem antiga escondia os eventos novos.
                $ultimaVista = @{}
                foreach ($a in $alvos) { $ultimaVista[$a.Serial] = $null }
                $ultima = [datetime]::MinValue
                $n = 0
                while (-not (Test-Path -LiteralPath $parar)) {
                    $eventos = @()
                    foreach ($a in $alvos) {
                        try {
                            $linhas = @(& $adb -s $a.Serial logcat -d -v time -s flutter 2>$null | Where-Object { $_ -match 'Kryptos:' })
                            if ($linhas.Count -gt 0) {
                                $inicio = 0
                                $vistaAntes = $ultimaVista[$a.Serial]
                                if ($null -ne $vistaAntes) {
                                    $pos = [array]::LastIndexOf($linhas, $vistaAntes)
                                    # Achou: so o que veio depois. Nao achou: o logcat recomecou e tudo e novo.
                                    if ($pos -ge 0) { $inicio = $pos + 1 }
                                }
                                for ($k = $inicio; $k -lt $linhas.Count; $k++) {
                                    $eventos += "$($a.Nome): " + ($linhas[$k] -replace '^.*Kryptos:\s*', '')
                                }
                                $ultimaVista[$a.Serial] = $linhas[$linhas.Count - 1]
                            }
                        } catch {
                            Add-Content -LiteralPath $registro -Value "falha ao ler o logcat de $($a.Nome): $($_.Exception.Message)"
                        }
                    }
                    $agora = Get-Date
                    if ($eventos.Count -gt 0 -or ($agora - $ultima).TotalSeconds -ge 15) {
                        # Depois de um evento, a tela leva um instante para redesenhar.
                        if ($eventos.Count -gt 0) { Start-Sleep -Milliseconds 800 }
                        $n++
                        $tipo = if ($eventos.Count -gt 0) { 'evento' } else { 'periodica' }
                        $hora = (Get-Date).ToString('HHmmss')
                        foreach ($a in $alvos) {
                            $arq = Join-Path $pasta ('{0:D3}-{1}-{2}-{3}.png' -f $n, $hora, $a.Nome, $tipo)
                            try {
                                & $adb -s $a.Serial shell screencap -p /sdcard/kryptos-captura.png 2>&1 | Out-Null
                                & $adb -s $a.Serial pull /sdcard/kryptos-captura.png $arq 2>&1 | Out-Null
                                & $adb -s $a.Serial shell rm -f /sdcard/kryptos-captura.png 2>&1 | Out-Null
                                if (-not (Test-Path -LiteralPath $arq)) { throw 'a captura nao chegou' }
                            } catch {
                                Add-Content -LiteralPath $registro -Value "falha ao capturar $($a.Nome): $($_.Exception.Message)"
                            }
                        }
                        $linha = ('{0:D3} {1} {2}' -f $n, (Get-Date).ToString('HH:mm:ss'), $tipo)
                        if ($eventos.Count -gt 0) { $linha += ' | ' + ($eventos -join ' | ') }
                        Add-Content -LiteralPath $registro -Value $linha
                        $ultima = Get-Date
                    }
                    Start-Sleep -Milliseconds 1500
                }
            }
            Write-Host "   capturando as telas sozinho em $pastaCapturas (a cada evento do app e a cada 15 s)"
        }
        $inicioDaEspera = Get-Date
        do { $resposta = Read-Host 'Digite FIM para encerrar' } while ($resposta.Trim().ToUpperInvariant() -ne 'FIM')
        $resultados.Add("OK    espera do teste manual: $([math]::Round(((Get-Date) - $inicioDaEspera).TotalMinutes, 1)) min")
        if ($jobCapturas) {
            New-Item -ItemType File -Path $pararCapturas -Force | Out-Null
            if (-not (Wait-Job -Job $jobCapturas -Timeout 30)) { Write-Host '   aviso: as capturas nao pararam em 30 s; parando a forca' }
            $saidaJob = @(Receive-Job -Job $jobCapturas -ErrorAction Continue 2>&1)
            foreach ($l in $saidaJob) { Write-Host "   capturas: $l" }
            Remove-Job -Job $jobCapturas -Force
            $script:jobCapturas = $null
            Remove-Item -LiteralPath $pararCapturas -Force
            $pngs = @(Get-ChildItem -LiteralPath $pastaCapturas -Filter *.png)
            $zipCapturas = "$pastaCapturas.zip"
            Compress-Archive -Path (Join-Path $pastaCapturas '*') -DestinationPath $zipCapturas
            $kb = [math]::Round((Get-Item -LiteralPath $zipCapturas).Length / 1KB)
            $resultados.Add("OK    capturas: $($pngs.Count) telas em $zipCapturas ($kb KB); roteiro em eventos.txt")
        }
        $resultados.Add('OK    teste na mao terminado (o resultado e o que voce viu nos celulares)')
        if ($SondaBle) {
            LerEAnalisarSonda
        }
        # O logcat foi limpo ao abrir o app: aqui so entra a sessao do teste.
        foreach ($a in @($aparelhos | Where-Object { -not $SondaBle })) {
            $log = @((Rodar $adb @('-s', $a.Serial, 'logcat', '-d', '-v', 'time', '-s', 'flutter')) -split "`n" |
                Where-Object { $_ -match '\S' -and $_ -notmatch '^-+ beginning of' })
            $resultados.Add("..    $($a.Modelo) : $($log.Count) linhas do app no logcat (ultimas 60)")
            foreach ($l in ($log | Select-Object -Last 60)) { $resultados.Add("      $($l.Trim())") }
        }
    }

    } # fim do else do -LerSonda

    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    if ($jobCapturas) {
        if ($pararCapturas) { New-Item -ItemType File -Path $pararCapturas -Force | Out-Null }
        Stop-Job -Job $jobCapturas
        Remove-Job -Job $jobCapturas -Force
        Write-Host 'capturas paradas'
    }
    foreach ($sRev in $comReverse) {
        $antes = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & $adb -s $sRev reverse --remove "tcp:$portaRelay" 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { Write-Host "   aviso: nao consegui desfazer o adb reverse em $sRev" }
        $ErrorActionPreference = $antes
    }
    if ($procRelay -and -not $procRelay.HasExited) {
        Stop-Process -Id $procRelay.Id -Force
        Write-Host 'relay de teste desligado'
    }
    foreach ($l in $logsRelay) { if (Test-Path -LiteralPath $l) { Remove-Item -LiteralPath $l -Force } }
    # O logcat em fluxo para aqui de qualquer jeito, e os brutos (com enderecos) saem.
    PararCapturasLogcat
    ApagarBrutosLogcat
    Set-Location -LiteralPath $Repo
    Write-Host ''
    foreach ($linha in $resultados) { Write-Host $linha }
    if ($ok) { Write-Host 'RESULTADO: OK' } else { Write-Host 'RESULTADO: FALHOU' }
    if ($transcricao) {
        try {
            Stop-Transcript | Out-Null
            LimparTranscricao $transcricao
            $coleta.Add($transcricao)
        } catch { Write-Host "aviso: transcricao: $($_.Exception.Message)" }
    }
    if ($coleta.Count -gt 0) {
        try {
            $zip = Join-Path $Amb.downloads "kryptos-sonda-$carimbo.zip"
            Compress-Archive -Path $coleta.ToArray() -DestinationPath $zip -Force
            $n = @((Get-ChildItem -LiteralPath $coleta.ToArray())).Count
            Write-Host "Para anexar: $zip ($n arquivos: medidas, logcat, analise e esta janela; $([math]::Round((Get-Item -LiteralPath $zip).Length / 1KB)) KB)"
        } catch { Write-Host "FALHA ao montar o zip: $($_.Exception.Message). Os arquivos estao em $($Amb.downloads)" }
    }
}

if ($ok) { exit 0 }
exit 1
