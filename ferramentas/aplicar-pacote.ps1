# ferramentas/aplicar-pacote.ps1
# Aplica um pacote de arquivos inteiros entregue pelo Claude (regra 3.1: mudanca
# grande em arquivo pequeno viaja como arquivo inteiro, com conferencia de sha).
#
# O zip traz os arquivos com o caminho do repositorio e um PACOTE.sha256, uma linha
# por arquivo:   <sha256 de antes ou NOVO>  <sha256 de depois>  <caminho>
#
# Antes de gravar qualquer coisa, confere:
#   - cada arquivo do zip bate com o sha de depois;
#   - cada arquivo do repositorio esta exatamente como o Claude o viu (sha de antes,
#     aceitando o mesmo conteudo com fim de linha CRLF),
#     ou ainda nao existe (NOVO), ou ja esta aplicado (sha de depois: pula).
# Se um unico arquivo nao bater, nada e gravado. Depois de gravar, le de volta.
# Nao faz commit.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\aplicar-pacote.ps1 <zip>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Zip,
    [string]$Raiz
)

$ErrorActionPreference = 'Stop'
if (-not $Raiz) {
    # Dentro do repositorio: a pasta acima de ferramentas. Fora (primeira vez, rodando
    # de Downloads): C:\kryptos.
    $acima = Split-Path -Parent $PSScriptRoot
    if (Test-Path -LiteralPath (Join-Path $acima '.git')) { $Raiz = $acima } else { $Raiz = 'C:\kryptos' }
}
if (-not (Test-Path -LiteralPath (Join-Path $Raiz '.git'))) { Write-Host "ERRO: $Raiz nao e o repositorio"; exit 1 }

$sep = [System.IO.Path]::DirectorySeparatorChar
$resultados = New-Object System.Collections.Generic.List[string]
$temporarios = New-Object System.Collections.Generic.List[string]

function Sha([string]$Caminho) { return (Get-FileHash -LiteralPath $Caminho -Algorithm SHA256).Hash }
function ShaLF([string]$Caminho) {
    # O mesmo arquivo com CRLF trocado por LF: o git do Windows pode ter convertido
    # o fim de linha, e isso nao e mudanca de conteudo.
    $b = [System.IO.File]::ReadAllBytes($Caminho)
    $saida = New-Object System.IO.MemoryStream
    for ($k = 0; $k -lt $b.Length; $k++) {
        if ($b[$k] -eq 13 -and ($k + 1) -lt $b.Length -and $b[$k + 1] -eq 10) { continue }
        $saida.WriteByte($b[$k])
    }
    $h = [System.Security.Cryptography.SHA256]::Create()
    try { return ([System.BitConverter]::ToString($h.ComputeHash($saida.ToArray()))).Replace('-', '') } finally { $h.Dispose() }
}
function Destino([string]$Rel) { return Join-Path $Raiz ($Rel.Replace('/', $sep)) }

$ok = $false
try {
    if (-not (Test-Path -LiteralPath $Zip)) { throw "nao achei $Zip" }
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('kryptos-' + [guid]::NewGuid().ToString('N'))
    $temporarios.Add($tmp)
    Expand-Archive -LiteralPath $Zip -DestinationPath $tmp
    $manifesto = Join-Path $tmp 'PACOTE.sha256'
    if (-not (Test-Path -LiteralPath $manifesto)) { throw 'zip sem PACOTE.sha256' }

    $itens = @()
    foreach ($l in (Get-Content -LiteralPath $manifesto -Encoding UTF8)) {
        if (-not $l.Trim()) { continue }
        if ($l -notmatch '^([0-9a-fA-F]{64}|NOVO)  ([0-9a-fA-F]{64})  (.+)$') { throw "linha estranha no PACOTE.sha256: $l" }
        $antes = $Matches[1]; if ($antes -ne 'NOVO') { $antes = $antes.ToUpperInvariant() }
        $itens += [pscustomobject]@{ Antes = $antes; Depois = $Matches[2].ToUpperInvariant(); Rel = $Matches[3] }
    }
    if ($itens.Count -eq 0) { throw 'PACOTE.sha256 vazio' }

    # ---------- conferir tudo antes de gravar ----------
    $gravar = @(); $jaAplicados = 0; $problemas = @()
    foreach ($i in $itens) {
        if ($i.Rel -match '(^|/)\.\.(/|$)' -or $i.Rel.StartsWith('/')) { $problemas += "$($i.Rel): caminho fora do repositorio"; continue }
        $origem = Join-Path $tmp ($i.Rel.Replace('/', $sep))
        if (-not (Test-Path -LiteralPath $origem)) { $problemas += "$($i.Rel): falta no zip"; continue }
        if ((Sha $origem) -ne $i.Depois) { $problemas += "$($i.Rel): arquivo do zip nao bate com o sha de depois"; continue }
        $d = Destino $i.Rel
        if (Test-Path -LiteralPath $d) {
            $atual = Sha $d
            if ($atual -eq $i.Depois) { $jaAplicados++; continue }
            if ($i.Antes -eq 'NOVO') { $problemas += "$($i.Rel): deveria ser novo, mas ja existe com outro conteudo"; continue }
            if ($atual -ne $i.Antes -and (ShaLF $d) -ne $i.Antes) { $problemas += "$($i.Rel): mudou desde que o Claude leu (sha atual $($atual.Substring(0,12)))"; continue }
        } elseif ($i.Antes -ne 'NOVO') { $problemas += "$($i.Rel): deveria existir e nao existe"; continue }
        $gravar += $i
    }
    if ($problemas.Count -gt 0) { throw "nada foi gravado:`n  $($problemas -join "`n  ")" }
    $resultados.Add("OK    conferido: $($gravar.Count) a gravar, $jaAplicados ja aplicado(s)")

    # ---------- gravar e ler de volta ----------
    foreach ($i in $gravar) {
        $d = Destino $i.Rel
        New-Item -ItemType Directory -Path (Split-Path -Parent $d) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $tmp ($i.Rel.Replace('/', $sep))) -Destination $d -Force
    }
    $errados = @($itens | Where-Object { (Sha (Destino $_.Rel)) -ne $_.Depois } | ForEach-Object { $_.Rel })
    if ($errados.Count -gt 0) { throw "depois de gravar, sha diferente em: $($errados -join ', ')" }
    foreach ($i in $gravar) { $resultados.Add("OK    $($i.Rel)") }
    $resultados.Add("OK    releitura: os $($itens.Count) arquivos do pacote batem com o sha de depois")
    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    foreach ($t in $temporarios) { if (Test-Path -LiteralPath $t) { Remove-Item -LiteralPath $t -Recurse -Force } }
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
