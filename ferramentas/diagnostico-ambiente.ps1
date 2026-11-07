# ferramentas/diagnostico-ambiente.ps1
# Confere, sem alterar nada, se a maquina tem tudo o que os projetos do Kryptos
# precisam: Rust (com o alvo Android), linker do Visual Studio, Flutter, SDK e NDK
# do Android, gerador do flutter_rust_bridge e o celular ligado por USB.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\diagnostico-ambiente.ps1
#
# Excecao registrada a regra 4.3 (parar no primeiro erro): este script e so
# leitura e confere tudo de uma vez, para a lista do que falta sair numa rodada
# so. Nenhum comando aqui instala, apaga ou grava arquivo.
#
# Saida: uma linha OK / FALTA / AVISO por item e, no fim, RESULTADO.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot

$linhas = New-Object System.Collections.Generic.List[string]
$faltas = 0

function Ok([string]$item, [string]$detalhe)    { $linhas.Add(("OK     {0,-28} {1}" -f $item, $detalhe)) }
function Falta([string]$item, [string]$detalhe) { $linhas.Add(("FALTA  {0,-28} {1}" -f $item, $detalhe)); $script:faltas++ }
function Aviso([string]$item, [string]$detalhe) { $linhas.Add(("AVISO  {0,-28} {1}" -f $item, $detalhe)) }

function Tentar([string]$Exe, [string[]]$Argumentos) {
    # Roda um comando externo e devolve o texto da saida, ou $null se o comando
    # nao existe ou saiu com codigo diferente de 0. O motivo vai para $script:ultimoErro.
    $script:ultimoErro = ''
    if (-not (Get-Command $Exe -ErrorAction SilentlyContinue)) { $script:ultimoErro = 'nao encontrado no PATH'; return $null }
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $saida = & $Exe @Argumentos 2>&1
        $codigo = $LASTEXITCODE
    } catch {
        $script:ultimoErro = $_.Exception.Message
        return $null
    } finally { $ErrorActionPreference = $antes }
    # ForEach com "$_": no PowerShell 5.1 o stderr vem como ErrorRecord, e Out-String
    # acrescentaria linhas de CategoryInfo ao texto.
    $texto = (@($saida | ForEach-Object { "$_" }) -join "`n").Trim()
    if ($codigo -ne 0) { $script:ultimoErro = "codigo $codigo"; return $null }
    return $texto
}

function PrimeiraLinha([string]$t) { if ($null -eq $t) { return '' } return (($t -split "`n")[0]).Trim() }

try {
    # ---------- git e repositorio ----------
    $v = Tentar 'git' @('--version')
    if ($v) { Ok 'git' ($v -replace '^git version ', '') } else { Falta 'git' $ultimoErro }
    $v = Tentar 'git' @('-C', $Repo, 'log', '-1', '--format=%h %s')
    if ($v) { Ok 'ultimo commit' $v } else { Aviso 'ultimo commit' "nao li o git em $Repo ($ultimoErro)" }

    # ---------- Rust ----------
    $v = Tentar 'rustup' @('--version')
    if ($v) { Ok 'rustup' (PrimeiraLinha $v) } else { Falta 'rustup' "$ultimoErro (instalar em https://rustup.rs)" }
    $v = Tentar 'rustc' @('--version')
    if ($v) { Ok 'rustc' $v } else { Falta 'rustc' $ultimoErro }
    $v = Tentar 'cargo' @('--version')
    if ($v) { Ok 'cargo' $v } else { Falta 'cargo' $ultimoErro }
    $v = Tentar 'rustup' @('show', 'active-toolchain')
    if ($v) {
        if ($v -match 'msvc') { Ok 'toolchain ativa' (PrimeiraLinha $v) }
        else { Aviso 'toolchain ativa' "$(PrimeiraLinha $v) (o esperado no Windows e *-pc-windows-msvc)" }
    }
    $v = Tentar 'rustup' @('target', 'list', '--installed')
    if ($null -ne $v) {
        $alvos = @($v -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ })
        if ($alvos -contains 'aarch64-linux-android') { Ok 'alvo aarch64-linux-android' 'instalado' }
        else { Falta 'alvo aarch64-linux-android' "nao instalado (instalados: $($alvos -join ', '))" }
    }

    # Linker do Visual Studio: sem ele o Rust no Windows nao gera executavel.
    $vswhere = $null
    if (${env:ProgramFiles(x86)}) { $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe' }
    if ($vswhere -and (Test-Path -LiteralPath $vswhere)) {
        $vs = Tentar $vswhere @('-latest', '-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'displayName')
        if ($vs) { Ok 'C++ do Visual Studio' (PrimeiraLinha $vs) }
        else { Falta 'C++ do Visual Studio' 'Visual Studio instalado sem o componente "Desenvolvimento para desktop com C++"' }
    } else { Falta 'C++ do Visual Studio' 'vswhere.exe nao encontrado (Build Tools do Visual Studio ausentes)' }

    $v = Tentar 'flutter_rust_bridge_codegen' @('--version')
    if ($v) { Ok 'flutter_rust_bridge_codegen' (PrimeiraLinha $v) } else { Falta 'flutter_rust_bridge_codegen' $ultimoErro }

    # ---------- Flutter ----------
    $v = Tentar 'flutter' @('--version', '--machine')
    if ($v) {
        $json = $null
        $inicio = $v.IndexOf('{')
        if ($inicio -ge 0) { try { $json = $v.Substring($inicio) | ConvertFrom-Json } catch { $json = $null } }
        if ($json) { Ok 'flutter' "$($json.frameworkVersion) canal $($json.channel), Dart $($json.dartSdkVersion)" }
        else { Ok 'flutter' (PrimeiraLinha $v) }
    } else { Falta 'flutter' $ultimoErro }

    $doc = Tentar 'flutter' @('doctor')
    if ($null -ne $doc) {
        foreach ($l in ($doc -split "`n")) {
            $t = $l.Trim()
            # A marca de "certo" muda conforme a pagina de codigo do console (pode
            # chegar como 1 a 3 caracteres estranhos); por isso so '!' e 'X' sao lidos.
            if ($t -match '^\[([^\]]{1,3})\]\s*(.+)$') {
                $marca = $Matches[1]; $texto = $Matches[2]
                if ($texto.Length -gt 70) { $texto = $texto.Substring(0, 70) + '...' }
                if ($marca -eq '!') { Aviso 'doctor' $texto }
                elseif ($marca -eq 'X' -or $marca -eq [string][char]0x2717) {
                    # X do doctor nem sempre importa (Chrome, Visual Studio para app desktop);
                    # o que o Kryptos precisa ja e conferido nos itens proprios acima e abaixo.
                    $linhas.Add(("INFO   {0,-28} [X] {1}" -f 'doctor', $texto))
                }
                else { Ok 'doctor' $texto }
            }
        }
    }

    # ---------- SDK e NDK do Android ----------
    $sdk = $null
    $candidatos = @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT)
    if ($env:LOCALAPPDATA) { $candidatos += (Join-Path $env:LOCALAPPDATA 'Android\Sdk') }
    foreach ($c in $candidatos) {
        if ($c -and (Test-Path -LiteralPath $c)) { $sdk = $c; break }
    }
    if ($sdk) {
        Ok 'SDK do Android' $sdk
        $ndkDir = Join-Path $sdk 'ndk'
        $ndks = @()
        if (Test-Path -LiteralPath $ndkDir) { $ndks = @(Get-ChildItem -LiteralPath $ndkDir -Directory | ForEach-Object { $_.Name }) }
        if ($ndks.Count -eq 0) { Falta 'NDK' "nenhum em $ndkDir" }
        else {
            # NDK r28 (28.x) em diante alinha as bibliotecas em 16 KB por padrao.
            $novos = @($ndks | Where-Object { [int](($_ -split '\.')[0]) -ge 28 })
            if ($novos.Count -gt 0) { Ok 'NDK' ($ndks -join ', ') }
            else { Falta 'NDK r28 ou mais novo' "so ha: $($ndks -join ', ')" }
        }
        $cmd = Join-Path $sdk 'cmdline-tools'
        if (Test-Path -LiteralPath $cmd) { Ok 'cmdline-tools' 'presente' } else { Falta 'cmdline-tools' "nao achei $cmd" }
    } else { Falta 'SDK do Android' 'nem ANDROID_HOME nem %LOCALAPPDATA%\Android\Sdk' }

    # ---------- Celular ----------
    $adb = $null
    if ($sdk -and (Test-Path -LiteralPath (Join-Path $sdk 'platform-tools\adb.exe'))) { $adb = Join-Path $sdk 'platform-tools\adb.exe' }
    elseif (Get-Command 'adb' -ErrorAction SilentlyContinue) { $adb = 'adb' }
    if (-not $adb) { Falta 'adb' 'platform-tools ausente' }
    else {
        $lista = Tentar $adb @('devices')
        $series = @()
        if ($lista) {
            foreach ($l in ($lista -split "`n" | Select-Object -Skip 1)) {
                $p = $l.Trim() -split '\s+'
                if ($p.Count -ge 2) {
                    if ($p[1] -eq 'device') { $series += $p[0] }
                    elseif ($p[1] -eq 'unauthorized') { Falta 'celular' "$($p[0]) sem autorizacao: aceite a chave RSA na tela do celular" }
                    else { Aviso 'celular' "$($p[0]) em estado '$($p[1])'" }
                }
            }
        }
        if ($series.Count -eq 0) { Falta 'celular' 'nenhum aparelho autorizado no adb (cabo, Depuracao USB ligada?)' }
        foreach ($s in $series) {
            $props = [ordered]@{
                'modelo'        = 'ro.product.model'
                'android'       = 'ro.build.version.release'
                'api'           = 'ro.build.version.sdk'
                'abi'           = 'ro.product.cpu.abi'
                'hyperos'       = 'ro.mi.os.version.name'
            }
            $partes = @()
            foreach ($k in $props.Keys) {
                $val = Tentar $adb @('-s', $s, 'shell', 'getprop', $props[$k])
                if ($val) { $partes += "$k=$val" }
            }
            $pg = Tentar $adb @('-s', $s, 'shell', 'getconf', 'PAGE_SIZE')
            if ($pg) { $partes += "pagina=$pg" }
            Ok "celular $s" ($partes -join '  ')
        }
    }

    # ---------- Disco ----------
    $letra = $null
    try { $letra = (Split-Path -Qualifier $Repo).TrimEnd(':') } catch { Aviso 'espaco livre' "nao identifiquei a unidade de $Repo" }
    $drv = $null
    if ($letra) { $drv = Get-PSDrive -Name $letra -ErrorAction SilentlyContinue }
    if ($drv) {
        $gb = [math]::Round($drv.Free / 1GB, 1)
        if ($gb -ge 15) { Ok "espaco livre em ${letra}:" "$gb GB" }
        else { Aviso "espaco livre em ${letra}:" "$gb GB (estimo uns 10 GB entre target do Rust e build do Android)" }
    }
} catch {
    $linhas.Add("FALHA  o proprio diagnostico quebrou: $($_.Exception.Message)")
    $faltas++
} finally {
    foreach ($l in $linhas) { Write-Host $l }
}

if ($faltas -eq 0) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host "RESULTADO: FALTAM $faltas item(ns)"
exit 1
