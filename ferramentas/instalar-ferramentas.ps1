# ferramentas/instalar-ferramentas.ps1
# Instala o que o diagnostico apontou como faltando para compilar o Kryptos no
# Windows: alvo Android do Rust, JDK 17, SDK e NDK do Android (so linha de comando,
# sem Android Studio), Flutter e o gerador do flutter_rust_bridge. No fim grava os
# caminhos no ambiente.json, le de volta e roda o diagnostico.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-ferramentas.ps1
#
# Pode rodar de novo: cada passo pula o que ja esta instalado.
# Para no primeiro erro. Pede interacao em dois pontos:
#   - janela do Windows (UAC) para instalar o JDK;
#   - aceite das licencas do SDK do Android (responder y).

[CmdletBinding()]
param(
    [string]$PastaFlutter = 'C:\dev\flutter'
)

# ---------- Versoes fixas (as duas maquinas ficam iguais) ----------
$CmdlineUrl    = 'https://dl.google.com/android/repository/commandlinetools-win-16111833_latest.zip'
$CmdlineSha256 = 'E5885E2E59038C0778A85FC19F01DE7CE7C567C105553F7617B936B1E0E87B2B'
$PacotesSdk    = @('platform-tools', 'platforms;android-36', 'build-tools;36.0.0', 'ndk;29.0.14206865')
$JdkWinget     = 'EclipseAdoptium.Temurin.17.JDK'
$FrbVersao     = '2.13.0'
$AlvoRust      = 'aarch64-linux-android'

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Repo = Split-Path -Parent $PSScriptRoot
$arqAmb = Join-Path $PSScriptRoot 'ambiente.json'
if (-not (Test-Path -LiteralPath $arqAmb)) {
    Write-Host 'ERRO: falta ferramentas\ambiente.json (rode ferramentas\criar-estrutura.ps1)'
    exit 1
}
$utf8 = New-Object System.Text.UTF8Encoding($false)

$resultados  = New-Object System.Collections.Generic.List[string]
$temporarios = New-Object System.Collections.Generic.List[string]

function Rodar([string]$Exe, [string[]]$Argumentos) {
    # Comando externo com saida capturada. Codigo != 0 derruba.
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & $Exe @Argumentos 2>&1 } finally { $ErrorActionPreference = $antes }
    $codigo = $LASTEXITCODE
    $texto = (@($saida | ForEach-Object { "$_" }) -join "`n").Trim()
    if ($codigo -ne 0) {
        $fim = (@($texto -split "`n") | Select-Object -Last 15) -join "`n"
        throw "$Exe $($Argumentos -join ' ') saiu com codigo $codigo.`n$fim"
    }
    return $texto
}

function Interativo([string]$Exe, [string[]]$Argumentos) {
    # Comando externo que precisa do teclado: saida direto na tela.
    & $Exe @Argumentos
    if ($LASTEXITCODE -ne 0) { throw "$Exe $($Argumentos -join ' ') saiu com codigo $LASTEXITCODE" }
}

function Passo([string]$Nome, [scriptblock]$Acao) {
    Write-Host ".. $Nome"
    $detalhe = & $Acao
    if ($detalhe) { $resultados.Add("OK    $Nome  ($detalhe)") } else { $resultados.Add("OK    $Nome") }
}

function SemBarra([string]$p) { return $p.Trim().TrimEnd('\').ToLowerInvariant() }

function Path-UsuarioAcrescentar([string[]]$Pastas) {
    # Le e grava o Path do usuario direto no registro, sem expandir %VARIAVEIS%
    # (o [Environment]::SetEnvironmentVariable trocaria %USERPROFILE% por texto fixo).
    $chave = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        $atual = [string]$chave.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $itens = @($atual -split ';' | Where-Object { $_.Trim() })
        $expandidos = @($itens | ForEach-Object { SemBarra ([Environment]::ExpandEnvironmentVariables($_)) })
        $novos = @($Pastas | Where-Object { $expandidos -notcontains (SemBarra $_) })
        if ($novos.Count -gt 0) {
            $chave.SetValue('Path', (($itens + $novos) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
        }
    } finally { $chave.Close() }
    foreach ($p in $Pastas) {
        $sessao = @($env:Path -split ';' | ForEach-Object { SemBarra $_ })
        if ($sessao -notcontains (SemBarra $p)) { $env:Path = "$p;$env:Path" }
    }
    return $novos.Count
}

function Variavel-Usuario([string]$Nome, [string]$Valor) {
    # SetEnvironmentVariable no escopo User avisa o Windows da mudanca (inclusive do
    # Path gravado acima), entao terminais abertos depois ja enxergam tudo.
    [Environment]::SetEnvironmentVariable($Nome, $Valor, 'User')
    Set-Item -Path "Env:$Nome" -Value $Valor
}

$ok = $false
try {
    Set-Location -LiteralPath $Repo
    $Amb = Get-Content -LiteralPath $arqAmb -Raw -Encoding UTF8 | ConvertFrom-Json
    $sdk = Join-Path $env:LOCALAPPDATA 'Android\Sdk'

    # ===== 1. Alvo Android do Rust =====
    Passo "alvo Rust $AlvoRust" {
        $inst = @((Rodar 'rustup' @('target', 'list', '--installed')) -split "`n" | ForEach-Object { $_.Trim() })
        if ($inst -contains $AlvoRust) { 'ja estava' }
        else { Rodar 'rustup' @('target', 'add', $AlvoRust) | Out-Null; 'instalado' }
    }

    # ===== 2. JDK 17 =====
    $raizJdk = Join-Path $env:ProgramFiles 'Eclipse Adoptium'
    function Achar-Jdk17 {
        if (-not (Test-Path -LiteralPath $raizJdk)) { return $null }
        $d = Get-ChildItem -LiteralPath $raizJdk -Directory -Filter 'jdk-17*' | Sort-Object Name -Descending | Select-Object -First 1
        if ($d -and (Test-Path -LiteralPath (Join-Path $d.FullName 'bin\java.exe'))) { return $d.FullName }
        return $null
    }
    $jdk = $null
    Passo 'JDK 17 (Temurin)' {
        $script:jdk = Achar-Jdk17
        $como = 'ja estava'
        if (-not $script:jdk) {
            if (-not (Get-Command 'winget' -ErrorAction SilentlyContinue)) { throw 'winget nao encontrado (instale o "Instalador de Aplicativo" pela Microsoft Store)' }
            Write-Host '   (vai abrir a janela do Windows pedindo permissao)'
            Rodar 'winget' @('install', '--id', $JdkWinget, '--exact', '--source', 'winget', '--silent',
                             '--accept-package-agreements', '--accept-source-agreements') | Out-Null
            $script:jdk = Achar-Jdk17
            if (-not $script:jdk) { throw "winget terminou, mas nao achei jdk-17* em $raizJdk" }
            $como = 'instalado'
        }
        $v = Rodar (Join-Path $script:jdk 'bin\java.exe') @('-version')
        if ($v -notmatch 'version "17\.') { throw "java em $script:jdk nao e 17: $v" }
        "$como, $script:jdk"
    }
    Variavel-Usuario 'JAVA_HOME' $jdk

    # ===== 3. Ferramentas de linha de comando do SDK do Android =====
    $sdkmanager = Join-Path $sdk 'cmdline-tools\latest\bin\sdkmanager.bat'
    Passo 'cmdline-tools do Android' {
        if (Test-Path -LiteralPath $sdkmanager) { return 'ja estava' }
        $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('kryptos-' + [guid]::NewGuid().ToString('N'))
        $temporarios.Add($tmp)
        New-Item -ItemType Directory -Path $tmp | Out-Null
        $zip = Join-Path $tmp 'cmdline-tools.zip'
        Write-Host '   baixando ~155 MB de dl.google.com'
        Rodar 'curl.exe' @('-fsSL', '--retry', '2', '-o', $zip, $CmdlineUrl) | Out-Null
        $h = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
        if ($h -ne $CmdlineSha256) { throw "sha256 do cmdline-tools nao confere: $h" }
        Expand-Archive -LiteralPath $zip -DestinationPath $tmp
        $destino = Join-Path $sdk 'cmdline-tools'
        New-Item -ItemType Directory -Path $destino -Force | Out-Null
        Move-Item -LiteralPath (Join-Path $tmp 'cmdline-tools') -Destination (Join-Path $destino 'latest')
        if (-not (Test-Path -LiteralPath $sdkmanager)) { throw "extrai, mas nao achei $sdkmanager" }
        'instalado, sha256 conferido'
    }
    Variavel-Usuario 'ANDROID_HOME' $sdk

    # ===== 4. Licencas e pacotes do SDK =====
    Passo 'licencas do SDK' {
        Write-Host '   responda y em cada licenca (se ja aceitou, so confirma)'
        Interativo $sdkmanager @('--licenses')
        'aceitas'
    }
    Passo 'pacotes do SDK' {
        Write-Host "   $($PacotesSdk -join ', ') (o NDK tem ~830 MB; demora)"
        # Lista em arquivo (--package_file): nomes com ';' passados como argumento a um
        # .bat dependem de como o cmd repassa a linha, e o arquivo elimina essa duvida.
        $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('kryptos-' + [guid]::NewGuid().ToString('N'))
        $temporarios.Add($tmp)
        New-Item -ItemType Directory -Path $tmp | Out-Null
        $lista = Join-Path $tmp 'pacotes.txt'
        [System.IO.File]::WriteAllText($lista, (($PacotesSdk -join "`n") + "`n"), $utf8)
        Rodar $sdkmanager @("--package_file=$lista") | Out-Null
        $faltam = @()
        if (-not (Test-Path -LiteralPath (Join-Path $sdk 'platform-tools\adb.exe'))) { $faltam += 'platform-tools' }
        if (-not (Test-Path -LiteralPath (Join-Path $sdk 'platforms\android-36'))) { $faltam += 'android-36' }
        if (-not (Test-Path -LiteralPath (Join-Path $sdk 'build-tools\36.0.0'))) { $faltam += 'build-tools 36.0.0' }
        if (-not (Test-Path -LiteralPath (Join-Path $sdk 'ndk\29.0.14206865'))) { $faltam += 'ndk 29.0.14206865' }
        if ($faltam.Count -gt 0) { throw "sdkmanager terminou sem instalar: $($faltam -join ', ')" }
        'conferidos no disco'
    }

    # ===== 5. Flutter =====
    $flutter = Join-Path $PastaFlutter 'bin\flutter.bat'
    Passo 'Flutter (canal stable)' {
        if (Test-Path -LiteralPath $PastaFlutter) {
            if (-not (Test-Path -LiteralPath $flutter)) { throw "$PastaFlutter existe mas nao e o Flutter. Nada foi alterado nela." }
            $como = 'ja estava'
        } else {
            New-Item -ItemType Directory -Path (Split-Path -Parent $PastaFlutter) -Force | Out-Null
            Write-Host '   git clone do Flutter (cerca de 1 GB; demora)'
            Rodar 'git' @('clone', '--branch', 'stable', 'https://github.com/flutter/flutter.git', $PastaFlutter) | Out-Null
            $como = 'clonado'
        }
        $n = Path-UsuarioAcrescentar @((Join-Path $PastaFlutter 'bin'), (Join-Path $sdk 'platform-tools'))
        Write-Host '   primeira execucao baixa o SDK do Dart'
        $v = Rodar $flutter @('--version', '--machine')
        $i = $v.IndexOf('{')
        if ($i -lt 0) { throw "flutter --version --machine nao devolveu JSON: $v" }
        $j = $v.Substring($i) | ConvertFrom-Json
        "$como, $($j.frameworkVersion), Dart $($j.dartSdkVersion), $n pasta(s) nova(s) no Path"
    }
    Passo 'Flutter apontado para SDK e JDK' {
        Rodar $flutter @('config', '--android-sdk', $sdk) | Out-Null
        Rodar $flutter @('config', '--jdk-dir', $jdk) | Out-Null
        'flutter config'
    }
    Passo 'telemetria do Flutter e do Dart desligada' {
        Rodar $flutter @('--disable-analytics') | Out-Null
        Rodar (Join-Path $PastaFlutter 'bin\dart.bat') @('--disable-analytics') | Out-Null
        'nao envia uso ao Google'
    }

    # ===== 6. Gerador do flutter_rust_bridge =====
    Passo "flutter_rust_bridge_codegen $FrbVersao" {
        $atual = $null
        if (Get-Command 'flutter_rust_bridge_codegen' -ErrorAction SilentlyContinue) {
            $atual = Rodar 'flutter_rust_bridge_codegen' @('--version')
        }
        if ($atual -and $atual -match [regex]::Escape($FrbVersao)) { return 'ja estava' }
        Write-Host '   cargo install compila o gerador (estimo 5 a 15 minutos)'
        Rodar 'cargo' @('install', 'flutter_rust_bridge_codegen', '--version', $FrbVersao, '--locked') | Out-Null
        $v = Rodar 'flutter_rust_bridge_codegen' @('--version')
        if ($v -notmatch [regex]::Escape($FrbVersao)) { throw "instalado, mas responde: $v" }
        'instalado'
    }

    # ===== 7. ambiente.json (grava e le de volta) =====
    Passo 'ambiente.json atualizado' {
        $campos = [ordered]@{
            flutter      = $PastaFlutter
            android_sdk  = $sdk
            jdk          = $jdk
            ndk          = '29.0.14206865'
        }
        foreach ($k in $campos.Keys) { $Amb | Add-Member -NotePropertyName $k -NotePropertyValue $campos[$k] -Force }
        [System.IO.File]::WriteAllText($arqAmb, (($Amb | ConvertTo-Json) + "`n"), $utf8)
        $lido = Get-Content -LiteralPath $arqAmb -Raw -Encoding UTF8 | ConvertFrom-Json
        foreach ($k in $campos.Keys) {
            if ($lido.$k -ne $campos[$k]) { throw "ambiente.json lido de volta: $k = '$($lido.$k)', esperado '$($campos[$k])'" }
        }
        "$($campos.Count) campos conferidos na releitura"
    }

    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    foreach ($t in $temporarios) {
        if (Test-Path -LiteralPath $t) { Remove-Item -LiteralPath $t -Recurse -Force }
    }
    Write-Host ''
    foreach ($linha in $resultados) { Write-Host $linha }
}

if (-not $ok) { Write-Host 'RESULTADO: FALHOU'; exit 1 }

# Prova: o diagnostico roda num processo filho, que herda o Path novo desta sessao.
Write-Host ''
Write-Host '== diagnostico =='
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'diagnostico-ambiente.ps1')
if ($LASTEXITCODE -ne 0) { Write-Host 'RESULTADO: INSTALADO, MAS O DIAGNOSTICO AINDA APONTA FALTA'; exit 1 }
Write-Host 'RESULTADO: OK'
exit 0
