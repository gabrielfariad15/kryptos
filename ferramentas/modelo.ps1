# ferramentas/modelo.ps1
# MODELO de script do Kryptos. Para um script novo: copie com outro nome e troque
# so o miolo (entre as linhas MIOLO). Do jeito que esta, ele confere o estado do
# repositorio e serve de exemplo.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\modelo.ps1
#
# O que o modelo garante:
#   - para no primeiro erro (comando externo com codigo diferente de 0 tambem para);
#   - limpeza no finally, rode o que rodar;
#   - imprime so linhas de resultado: OK / FALHA / RESULTADO;
#   - le os fatos da maquina de ferramentas\ambiente.json (fora do git).

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

$resultados  = New-Object System.Collections.Generic.List[string]
$temporarios = New-Object System.Collections.Generic.List[string]

function Rodar([string]$Exe, [string[]]$Argumentos) {
    # Comando externo. Avisos no stderr nao derrubam; codigo de saida != 0 derruba.
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & $Exe @Argumentos 2>&1 } finally { $ErrorActionPreference = $antes }
    $codigo = $LASTEXITCODE
    $texto = ($saida | Out-String).Trim()
    if ($codigo -ne 0) { throw "$Exe $($Argumentos -join ' ') saiu com codigo $codigo. $texto" }
    return $texto
}

function Passo([string]$Nome, [scriptblock]$Acao) {
    $detalhe = & $Acao
    if ($detalhe) { $resultados.Add("OK    $Nome  ($detalhe)") } else { $resultados.Add("OK    $Nome") }
}

$ok = $false
try {
    Set-Location -LiteralPath $Repo

    # ===== MIOLO: troque daqui =====
    Passo 'git disponivel' { (Rodar git @('--version')) -replace '^git version ', '' }
    Passo 'ramo atual' { Rodar git @('branch', '--show-current') }
    Passo 'mudancas nao salvas' {
        $st = Rodar git @('status', '--short')
        if ($st) { "$(@($st -split "`n").Count) arquivo(s)" } else { 'nenhuma' }
    }
    Passo 'ambiente.json lido' { "downloads = $($Amb.downloads)" }
    # Exemplo de temporario com limpeza garantida:
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("kryptos-" + [guid]::NewGuid().ToString('N'))
    $temporarios.Add($tmp)
    New-Item -ItemType Directory -Path $tmp | Out-Null
    Passo 'temporario criado' { 'sera apagado no final' }
    # ===== ate aqui =====

    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    foreach ($t in $temporarios) {
        if (Test-Path -LiteralPath $t) { Remove-Item -LiteralPath $t -Recurse -Force }
    }
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
