# ferramentas/instalar-no-celular.ps1
# Compila o app (debug, arm64), instala no celular ligado no cabo, abre e tira
# uma captura da tela. A captura e a prova de que a tela abriu (regra 5.5).
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1
#
# Saida: a captura vai para Downloads com nome unico (kryptos-tela-AAAAMMDD-HHMMSS.png).
# Quando o bloqueio de print (FLAG_SECURE) entrar no app, a captura vai sair preta:
# ai a prova passa a ser so a conferencia a olho.

[CmdletBinding()]
param()

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
    $flutter = Achar (Join-Path $Amb.flutter 'bin') @('flutter.bat', 'flutter')
    $adb = Achar (Join-Path $Amb.android_sdk 'platform-tools') @('adb.exe', 'adb')
    $app = Join-Path (Join-Path $Repo 'frontend') 'app'

    $serial = $null
    Passo 'celular no adb' {
        $s = @((Rodar $adb @('devices')) -split "`n" | Select-Object -Skip 1 |
            ForEach-Object { $p = $_.Trim() -split '\s+'; if ($p.Count -ge 2 -and $p[1] -eq 'device') { $p[0] } })
        if ($s.Count -ne 1) { throw "preciso de exatamente 1 celular autorizado no adb; achei $($s.Count)" }
        $script:serial = $s[0]
        $modelo = Rodar $adb @('-s', $s[0], 'shell', 'getprop', 'ro.product.model')
        $android = Rodar $adb @('-s', $s[0], 'shell', 'getprop', 'ro.build.version.release')
        "$modelo, Android $android"
    }

    $apk = Join-Path $app 'build\app\outputs\flutter-apk\app-debug.apk'
    Passo 'compilar APK de debug (arm64)' {
        Set-Location -LiteralPath $app
        Write-Host '   Gradle + Rust para Android; a primeira vez demora'
        Rodar $flutter @('build', 'apk', '--debug', '--target-platform', 'android-arm64') | Out-Null
        if (-not (Test-Path -LiteralPath $apk)) { throw "o build terminou mas nao achei $apk" }
        "$([math]::Round((Get-Item -LiteralPath $apk).Length / 1MB, 1)) MB"
    }

    Passo 'instalar' {
        Write-Host '   no HyperOS pode aparecer um aviso na tela do celular: permita'
        $t = Rodar $adb @('-s', $serial, 'install', '-r', $apk)
        if ($t -notmatch 'Success') { throw "adb install nao disse Success: $t" }
        'Success'
    }

    Passo 'abrir o app' {
        try { Rodar $adb @('-s', $serial, 'logcat', '-c') | Out-Null } catch { Write-Host "   aviso: nao limpei o logcat; a conferencia pode pegar erro antigo" }
        Rodar $adb @('-s', $serial, 'shell', 'am', 'start', '-W', '-n', "$Pacote/.MainActivity") | Out-Null
        Start-Sleep -Seconds 5
        $processo = $null
        try { $processo = Rodar $adb @('-s', $serial, 'shell', 'pidof', $Pacote) } catch { $processo = $null }
        if (-not $processo) {
            $log = Rodar $adb @('-s', $serial, 'logcat', '-d', '-t', '200', '*:E')
            throw "o app nao ficou aberto. Ultimos erros do logcat:`n$log"
        }
        "processo $processo"
    }

    Passo 'sem erro fatal no logcat' {
        $log = Rodar $adb @('-s', $serial, 'logcat', '-d', '-t', '2000')
        $ruins = @($log -split "`n" | Where-Object { $_ -match 'FATAL EXCEPTION|UnsatisfiedLinkError|PanicException|E/flutter|\[ERROR:flutter' })
        if ($ruins.Count -gt 0) { throw "logcat com erro:`n$($ruins | Select-Object -First 15 | Out-String)" }
        'nenhum'
    }

    Passo 'captura da tela' {
        $destino = Join-Path $Amb.downloads ('kryptos-tela-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.png')
        $remoto = '/sdcard/kryptos-tela.png'
        Rodar $adb @('-s', $serial, 'shell', 'screencap', '-p', $remoto) | Out-Null
        try { Rodar $adb @('-s', $serial, 'pull', $remoto, $destino) | Out-Null }
        finally { try { Rodar $adb @('-s', $serial, 'shell', 'rm', '-f', $remoto) | Out-Null } catch { Write-Host "   aviso: nao apaguei $remoto do celular: $($_.Exception.Message)" } }
        $b = [System.IO.File]::ReadAllBytes($destino)
        if ($b.Length -lt 8 -or $b[0] -ne 0x89 -or $b[1] -ne 0x50 -or $b[2] -ne 0x4E -or $b[3] -ne 0x47) { throw "$destino nao e um PNG valido" }
        "$destino, $([math]::Round($b.Length / 1KB)) KB"
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
