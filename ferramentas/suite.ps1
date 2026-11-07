# ferramentas/suite.ps1
# Suite do Kryptos: tudo o que precisa estar verde antes de um commit (regra 5.1).
# Ordem: fim de linha, Rust (fmt, clippy, testes), Flutter (analise, testes).
# Para no primeiro passo que falhar.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\suite.ps1
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\suite.ps1 -Aparelho
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\suite.ps1 -Aparelho -Todos
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\suite.ps1 -Aparelho -Serial <serial>
#
# -Aparelho tambem roda os testes no celular ligado no cabo (instala o app de teste,
# confere que o nucleo em Rust responde de dentro do Android, prova o cofre da
# identidade com o Keystore de verdade e o QR do contato pela ponte).
# Sem -Todos nem -Serial, exige um unico celular no cabo. -Todos roda em cada um, um
# depois do outro; -Serial escolhe um.

[CmdletBinding()]
param(
    [switch]$Aparelho,
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
    $inicio = Get-Date
    $detalhe = & $Acao
    $s = [math]::Round(((Get-Date) - $inicio).TotalSeconds)
    if ($detalhe) { $resultados.Add("OK    $Nome  ($detalhe; ${s}s)") } else { $resultados.Add("OK    $Nome  (${s}s)") }
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

function ContarTestes([string]$texto, [string]$padrao) {
    $total = 0
    foreach ($m in [regex]::Matches($texto, $padrao)) { $total += [int]$m.Groups[1].Value }
    return $total
}

$ok = $false
try {
    if ($Todos -and $Serial) { throw 'use -Todos ou -Serial, nao os dois' }
    if (($Todos -or $Serial) -and -not $Aparelho) { throw '-Todos e -Serial so valem junto com -Aparelho' }
    Set-Location -LiteralPath $Repo
    $flutter = Achar (Join-Path $Amb.flutter 'bin') @('flutter.bat', 'flutter')
    $app = Join-Path (Join-Path $Repo 'frontend') 'app'

    # ---------- Fim de linha (regra do .gitattributes) ----------
    Passo 'fim de linha' {
        $ps = (Get-Process -Id $PID).Path
        $t = Rodar $ps @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $PSScriptRoot 'conferir-fim-de-linha.ps1'), '-Raiz', $Repo)
        if ($t -match 'OK\s+fim de linha: (\d+) arquivos') { "$($Matches[1]) arquivos de texto" }
    }

    # ---------- Versao do Rust: a mesma na suite e no celular ----------
    Passo 'versao do Rust' {
        $ps = (Get-Process -Id $PID).Path
        $t = Rodar $ps @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $PSScriptRoot 'conferir-rust.ps1'), '-Raiz', $Repo)
        if ($t -match 'OK\s+versao do Rust: (\S+)') { "$($Matches[1]) na suite e no celular" }
    }

    # ---------- Rust (nucleo, relay e ponte) ----------
    Passo 'cargo fmt' { Rodar 'cargo' @('fmt', '--all', '--', '--check') | Out-Null; 'formatacao em dia' }
    Passo 'cargo clippy' { Rodar 'cargo' @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings') | Out-Null; 'sem aviso' }
    Passo 'cargo test' {
        $t = Rodar 'cargo' @('test', '--workspace')
        $n = ContarTestes $t 'test result: ok\. (\d+) passed'
        if ($n -eq 0) { throw 'cargo test passou, mas nenhum teste rodou' }
        "$n testes"
    }

    # ---------- Flutter ----------
    Set-Location -LiteralPath $app
    Passo 'flutter analyze' {
        $t = Rodar $flutter @('analyze')
        if ($t -notmatch 'No issues found') { throw "flutter analyze sem 'No issues found'" }
        'sem problema'
    }
    Passo 'flutter test' {
        $t = Rodar $flutter @('test')
        if ($t -notmatch 'All tests passed') { throw "flutter test sem 'All tests passed'" }
        $m = [regex]::Matches($t, '\+(\d+): All tests passed')
        if ($m.Count -gt 0) { "$($m[$m.Count - 1].Groups[1].Value) testes" }
    }

    if ($Aparelho) {
        $adb = Achar (Join-Path $Amb.android_sdk 'platform-tools') @('adb.exe', 'adb')
        # $Serial e o parametro; o PowerShell nao diferencia maiusculas, entao as
        # variaveis daqui tem outros nomes ($ligados, $alvos, $sAtual).
        $ligados = @((Rodar $adb @('devices')) -split "`n" | Select-Object -Skip 1 |
            ForEach-Object { $p = $_.Trim() -split '\s+'; if ($p.Count -ge 2 -and $p[1] -eq 'device') { $p[0] } })
        if ($ligados.Count -eq 0) { throw 'nenhum celular autorizado no adb' }
        if ($Serial) {
            if ($ligados -notcontains $Serial) { throw "serial $Serial nao esta no adb; ligados: $($ligados -join ', ')" }
            $alvos = @($Serial)
        } elseif ($Todos) {
            $alvos = $ligados
        } else {
            if ($ligados.Count -ne 1) { throw "ha $($ligados.Count) celulares ligados ($($ligados -join ', ')). Use -Todos ou -Serial <serial>" }
            $alvos = $ligados
        }
        foreach ($sAtual in $alvos) {
            $modelo = Rodar $adb @('-s', $sAtual, 'shell', 'getprop', 'ro.product.model')
            Passo "teste no celular: ponte, cofre e QR ($modelo)" {
                Write-Host "   $modelo [$sAtual]: compila o Rust para Android e instala o app de teste (a primeira vez demora)"
                $t = Rodar $flutter @('test', 'integration_test', '-d', $sAtual)
                if ($t -notmatch 'All tests passed') { throw "teste no aparelho sem 'All tests passed'" }
                # cofre_test.dart e qr_test.dart imprimem tempos, onde ficou a chave e a
                # varredura (sem segredo).
                $medidas = @([regex]::Matches($t, 'MEDIDA ([^\r\n]+)') | ForEach-Object { $_.Groups[1].Value.Trim() })
                if ($medidas.Count -lt 2) { throw 'o teste do cofre passou sem imprimir as linhas MEDIDA' }
                if (-not ($medidas | Where-Object { $_ -like 'qr no aparelho*' })) { throw 'o teste do QR passou sem imprimir a linha MEDIDA' }
                'nucleo 0.1.0 dentro do Android; ' + ($medidas -join '; ')
            }
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
