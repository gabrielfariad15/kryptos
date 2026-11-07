# ferramentas/instalar-no-celular.ps1
# Compila o app (debug, arm64) uma vez, instala no(s) celular(es) ligado(s) no cabo,
# abre, confere o logcat e tira uma captura da tela de cada um. A captura e a prova
# de que a tela abriu (regra 5.5). Tambem informa tamanho e densidade da tela, que
# entram nos testes de layout.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Serial <serial>
#
# Sem parametro: exige um unico celular no cabo. Com mais de um, use -Todos ou -Serial.
#
# Saida: capturas em Downloads com nome unico (kryptos-tela-<modelo>-AAAAMMDD-HHMMSS.png).
# Quando o bloqueio de print (FLAG_SECURE) entrar no app, a captura vai sair preta:
# ai a prova passa a ser so a conferencia a olho.

[CmdletBinding()]
param(
    [switch]$Todos,
    [string]$Serial
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

    # ---------- compilar uma vez ----------
    $apk = Join-Path $app 'build\app\outputs\flutter-apk\app-debug.apk'
    Passo 'compilar APK de debug (arm64)' {
        Set-Location -LiteralPath $app
        Write-Host '   Gradle + Rust para Android; a primeira vez demora'
        Rodar $flutter @('build', 'apk', '--debug', '--target-platform', 'android-arm64') | Out-Null
        if (-not (Test-Path -LiteralPath $apk)) { throw "o build terminou mas nao achei $apk" }
        "$([math]::Round((Get-Item -LiteralPath $apk).Length / 1MB, 1)) MB"
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
            "$destino, $([math]::Round($b.Length / 1KB)) KB"
        }
    }

    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    Set-Location -LiteralPath $Repo
    Write-Host ''
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
