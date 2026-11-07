# ferramentas/medir-leitura-qr.ps1
# Mede a leitura do QR do contato pela camera, no(s) celular(es) ligado(s) no cabo.
# Regra (cartao-de-contato.md): 9 em 10 leituras em menos de 2 s. Abaixo disso, a
# primeira variavel a mexer e a resolucao da camera (camera_do_aparelho.dart); a
# troca do leitor pelo flutter_zxing vem depois (DEPENDENCIAS.md).
#
# Como funciona: limpa o logcat, espera voce fazer as leituras e conta as linhas
# "MEDIDA qr" que o app escreve (leitor_de_qr.dart). Cada toque em LER QR DO CONTATO
# e uma tentativa: termina lida (o contato apareceu) ou cancelada (CANCELAR, voltar,
# app para o fundo). Ler de novo quem ja esta na agenda conta como leitura.
# Nada do QR vai para o logcat: so tempo e contagem de quadros.
#
# Uso (uma linha; o app ja instalado pelo instalar-no-celular.ps1):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\medir-leitura-qr.ps1
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\medir-leitura-qr.ps1 -Todos
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\medir-leitura-qr.ps1 -Serial <serial>
#
# -Leituras: quantas tentativas cada celular tem de ter (padrao 10).
# -ArquivoDeLog: conta um logcat ja salvo, sem adb (conferencia da contagem).

[CmdletBinding()]
param(
    [switch]$Todos,
    [string]$Serial,
    [int]$Leituras = 10,
    [int]$LimiteMs = 2000,
    [string]$ArquivoDeLog
)

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot

$resultados = New-Object System.Collections.Generic.List[string]

function Rodar([string]$Exe, [string[]]$Argumentos) {
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & $Exe @Argumentos 2>&1 } finally { $ErrorActionPreference = $antes }
    $codigo = $LASTEXITCODE
    $texto = (@($saida | ForEach-Object { "$_" }) -join "`n").Trim()
    if ($codigo -ne 0) { throw "$([System.IO.Path]::GetFileName($Exe)) $($Argumentos -join ' ') saiu com codigo $codigo. $texto" }
    return $texto
}

function Achar([string]$Pasta, [string[]]$Nomes) {
    if ($env:OS -ne 'Windows_NT') { $Nomes = @($Nomes | ForEach-Object { [System.IO.Path]::GetFileNameWithoutExtension($_) }) }
    foreach ($n in $Nomes) {
        $c = Join-Path $Pasta $n
        if (Test-Path -LiteralPath $c) { return $c }
    }
    throw "nao achei $($Nomes -join ' / ') em $Pasta"
}

function LerMedidas([string]$Log) {
    $padrao = 'MEDIDA qr (lido|cancelado)[^\r\n]*? em (\d+) ms, (\d+) quadros lidos, (\d+) descartados, nucleo (\d+) ms'
    return @([regex]::Matches($Log, $padrao) | ForEach-Object {
        [pscustomobject]@{
            Lido        = $_.Groups[1].Value -eq 'lido'
            Ms          = [int]$_.Groups[2].Value
            Quadros     = [int]$_.Groups[3].Value
            Descartados = [int]$_.Groups[4].Value
            NucleoMs    = [int]$_.Groups[5].Value
        }
    })
}

function Mediana([int[]]$v) {
    if ($v.Count -eq 0) { return 0 }
    $o = @($v | Sort-Object)
    if ($o.Count % 2 -eq 1) { return $o[[int][math]::Floor($o.Count / 2)] }
    return [int](($o[$o.Count / 2 - 1] + $o[$o.Count / 2]) / 2)
}

# Devolve se passou na regra de 9 em 10 e o texto. Nunca lanca: um celular que falha
# nao impede a contagem do outro.
# @(...) em toda lista: no Windows PowerShell 5.1, uma funcao que devolve um item so
# devolve o objeto solto, e objeto solto nao tem .Count (07/11/2026: saiu "so  tentativa").
function Avaliar([string]$Nome, [string]$Log) {
    $m = @(LerMedidas $Log)
    if ($m.Count -eq 0) { return [pscustomobject]@{ Passou = $false; Texto = 'nenhuma linha MEDIDA qr no logcat. O app instalado e o desta versao?' } }
    if ($m.Count -lt $Leituras) { return [pscustomobject]@{ Passou = $false; Texto = "so $($m.Count) tentativa(s); faltam $($Leituras - $m.Count) para $Leituras" } }
    $rapidas = @($m | Where-Object { $_.Lido -and $_.Ms -lt $LimiteMs }).Count
    $lidas = @($m | Where-Object { $_.Lido })
    $medianaMs = Mediana @($lidas | ForEach-Object { $_.Ms })
    $nucleo = Mediana @($m | Where-Object { $_.Quadros -gt 0 } | ForEach-Object { $_.NucleoMs })
    $descarte = ($m | Measure-Object -Property Descartados -Sum).Sum
    $quadros = ($m | Measure-Object -Property Quadros -Sum).Sum
    $texto = "$rapidas de $($m.Count) lidas em menos de $LimiteMs ms; $($lidas.Count) lidas no total; " +
             "mediana das lidas $medianaMs ms; nucleo $nucleo ms por quadro (mediana); " +
             "$quadros quadros lidos, $descarte descartados"
    $passou = ($rapidas * 10 -ge $m.Count * 9)
    if (-not $passou) { $texto = "abaixo de 9 em 10. $texto" }
    return [pscustomobject]@{ Passou = $passou; Texto = $texto }
}

# Junta o resultado de um celular (ou do arquivo) e diz se passou.
function Registrar([string]$Nome, $r) {
    if ($r.Passou) { $resultados.Add("OK    ${Nome}: $($r.Texto)"); return $true }
    $resultados.Add("FALHA ${Nome}: $($r.Texto)")
    return $false
}

$ok = $false
try {
    if ($Todos -and $Serial) { throw 'use -Todos ou -Serial, nao os dois' }
    if ($Leituras -lt 1) { throw '-Leituras precisa ser 1 ou mais' }

    if ($ArquivoDeLog) {
        if (-not (Test-Path -LiteralPath $ArquivoDeLog)) { throw "nao achei $ArquivoDeLog" }
        $passou = Registrar 'arquivo' (Avaliar 'arquivo' (Get-Content -LiteralPath $ArquivoDeLog -Raw -Encoding UTF8))
    } else {
        $arqAmb = Join-Path $PSScriptRoot 'ambiente.json'
        if (-not (Test-Path -LiteralPath $arqAmb)) { throw 'falta ferramentas\ambiente.json (rode ferramentas\criar-estrutura.ps1)' }
        $Amb = Get-Content -LiteralPath $arqAmb -Raw -Encoding UTF8 | ConvertFrom-Json
        $adb = Achar (Join-Path $Amb.android_sdk 'platform-tools') @('adb.exe', 'adb')

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
        $modelos = @{}
        foreach ($sAtual in $alvos) {
            $modelos[$sAtual] = Rodar $adb @('-s', $sAtual, 'shell', 'getprop', 'ro.product.model')
            Rodar $adb @('-s', $sAtual, 'logcat', '-c') | Out-Null
        }
        Write-Host "logcat limpo em: $(@($alvos | ForEach-Object { $modelos[$_] }) -join ', ')"
        Write-Host "Faca $Leituras tentativas em cada celular: LER QR DO CONTATO, aponte para o QR do outro,"
        Write-Host 'espere o contato aparecer (ou CANCELAR se travar) e toque MOSTRAR MEU QR para a proxima.'
        [void](Read-Host 'Aperte Enter quando terminar')
        # Todos os celulares sao contados antes de dizer o resultado.
        $passou = $true
        foreach ($sAtual in $alvos) {
            $log = Rodar $adb @('-s', $sAtual, 'logcat', '-d', '-s', 'flutter')
            if (-not (Registrar $modelos[$sAtual] (Avaliar $modelos[$sAtual] $log))) { $passou = $false }
        }
    }
    $ok = $passou
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
