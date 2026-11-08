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
#
# Sem parametro: exige um unico celular no cabo. Com mais de um, use -Todos ou -Serial.
#
# -Relay: o teste com os dois celulares. Sobe o relay de teste nesta maquina (so em
# 127.0.0.1, malote na memoria), monta o app com o endereco dele, liga a porta a cada
# celular pelo adb reverse e fica esperando o Enter enquanto o teste e feito na mao.
# No fim desliga o relay, desfaz os reverses e mostra as linhas do app no logcat de cada
# celular (o app so grava codigos, nunca texto, chave ou codigo de contato: regra 8.2).
# Os celulares ficam no cabo o tempo todo.
#
# -TelaLivre: so para filmar e capturar no teste. Monta o app sem FLAG_SECURE (o Android
# so aceita em app de debug) e com uma faixa laranja "TELA LIBERADA" em todas as telas.
# A captura passa a ter de mostrar conteudo.
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
    [switch]$TelaLivre
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

$ok = $false
try {
    if ($Todos -and $Serial) { throw 'use -Todos ou -Serial, nao os dois' }
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
        $script:aparelhos = $escolhidos
        (@($escolhidos | ForEach-Object { "$($_.Modelo), Android $($_.Android)" }) -join ' | ')
    }

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

    # ---------- compilar uma vez ----------
    $apk = Join-Path $app 'build\app\outputs\flutter-apk\app-debug.apk'
    Passo 'compilar APK de debug (arm64)' {
        Set-Location -LiteralPath $app
        Write-Host '   Gradle + Rust para Android; a primeira vez demora'
        $argsBuild = @('build', 'apk', '--debug', '--target-platform', 'android-arm64')
        if ($Relay) { $argsBuild += "--dart-define=KRYPTOS_RELAY=http://127.0.0.1:$portaRelay" }
        if ($TelaLivre) { $argsBuild += '--dart-define=KRYPTOS_TELA_LIVRE=true' }
        Rodar $flutter $argsBuild | Out-Null
        if (-not (Test-Path -LiteralPath $apk)) { throw "o build terminou mas nao achei $apk" }
        $comRelay = if ($Relay) { 'com o relay de teste' } else { 'sem relay' }
        $comTela = if ($TelaLivre) { 'TELA LIVRE (sem FLAG_SECURE)' } else { 'tela segura' }
        "$([math]::Round((Get-Item -LiteralPath $apk).Length / 1MB, 1)) MB, $comRelay, $comTela"
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
            $sobrando = @('RECORD_AUDIO', 'WRITE_EXTERNAL_STORAGE', 'READ_EXTERNAL_STORAGE', 'FOREGROUND_SERVICE_CAMERA' |
                Where-Object { $pacoteInstalado -match "android\.permission\.$_\b" })
            if ($sobrando.Count -gt 0) { throw "o app instalado pede permissao que nao usa: $($sobrando -join ', ')" }
            'camera sim; microfone e armazenamento nao'
        }

        if ($Relay) {
            Passo "$modeloAtual : porta do relay" {
                Rodar $adb @('-s', $s, 'reverse', "tcp:$portaRelay", "tcp:$portaRelay") | Out-Null
                $comReverse.Add($s)
                "tcp:$portaRelay do celular -> tcp:$portaRelay desta maquina"
            }
        }

        Passo "$modeloAtual : abrir o app" {
            try { Rodar $adb @('-s', $s, 'logcat', '-c') | Out-Null } catch { Write-Host "   aviso: nao limpei o logcat; a conferencia pode pegar erro antigo" }
            Rodar $adb @('-s', $s, 'shell', 'am', 'start', '-W', '-n', "$Pacote/.MainActivity") | Out-Null
            Start-Sleep -Seconds 5
            $processo = $null
            try { $processo = Rodar $adb @('-s', $s, 'shell', 'pidof', $Pacote) } catch { $processo = $null }
            if (-not $processo) {
                $log = Rodar $adb @('-s', $s, 'logcat', '-d', '-t', '200', '*:E')
                throw "o app nao ficou aberto. Ultimos erros do logcat:`n$log"
            }
            "processo $processo"
        }

        Passo "$modeloAtual : sem erro fatal no logcat" {
            $log = Rodar $adb @('-s', $s, 'logcat', '-d', '-t', '2000')
            $ruins = @($log -split "`n" | Where-Object { $_ -match 'FATAL EXCEPTION|UnsatisfiedLinkError|PanicException|E/flutter|\[ERROR:flutter' })
            if ($ruins.Count -gt 0) { throw "logcat com erro:`n$($ruins | Select-Object -First 15 | Out-String)" }
            'nenhum'
        }

        Passo "$modeloAtual : captura da tela" {
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

    if ($Relay) {
        Write-Host ''
        Write-Host 'Relay de teste no ar. Faca o roteiro do teste com os dois celulares'
        Write-Host '(documentacao/arquitetura/telas-de-conversa.md, secao "Teste com os dois celulares")'
        Write-Host 'e aperte Enter aqui quando terminar. O relay desliga e os reverses saem.'
        [void](Read-Host)
        $resultados.Add('OK    teste na mao terminado (o resultado e o que voce viu nos celulares)')
        # O logcat foi limpo ao abrir o app: aqui so entra a sessao do teste.
        foreach ($a in $aparelhos) {
            $log = @((Rodar $adb @('-s', $a.Serial, 'logcat', '-d', '-v', 'time', '-s', 'flutter')) -split "`n" |
                Where-Object { $_ -match '\S' -and $_ -notmatch '^-+ beginning of' })
            $resultados.Add("..    $($a.Modelo) : $($log.Count) linhas do app no logcat (ultimas 60)")
            foreach ($l in ($log | Select-Object -Last 60)) { $resultados.Add("      $($l.Trim())") }
        }
    }

    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
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
    Set-Location -LiteralPath $Repo
    Write-Host ''
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
