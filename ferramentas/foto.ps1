# ferramentas/foto.ps1
# "Foto" do assunto: zipa de uma vez tudo que o assunto toca, para anexar na conversa.
# Deixa de fora o que nao deve sair da maquina ou so pesa: .git, target, build,
# .dart_tool, node_modules, .gradle, .kotlin, .cxx, segredos, .env, ambiente.json,
# chaves de assinatura.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\foto.ps1 nucleo backend\relay
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\foto.ps1        (repositorio inteiro)
#
# O zip vai para Downloads com nome unico: kryptos-foto-AAAAMMDD-HHMMSS.zip

param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Caminhos)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$Repo = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$arqAmb = Join-Path $PSScriptRoot 'ambiente.json'
if (-not (Test-Path -LiteralPath $arqAmb)) { Write-Host 'ERRO: falta ferramentas\ambiente.json'; exit 1 }
$Amb = Get-Content -LiteralPath $arqAmb -Raw -Encoding UTF8 | ConvertFrom-Json

# .gradle, .kotlin e .cxx: cache do Gradle, do Kotlin e do NDK dentro de android\.
$pastasFora   = @('.git', 'target', 'build', '.dart_tool', 'node_modules', 'segredos', '.gradle', '.kotlin', '.cxx')
$arquivosFora = @('.env', 'ambiente.json', 'key.properties', 'local.properties')
$extFora      = @('.jks', '.keystore')

if (-not $Caminhos -or $Caminhos.Count -eq 0) { $Caminhos = @('.') }

$naoAchei = @()
$escolhidos = New-Object 'System.Collections.Generic.SortedSet[string]'
$deFora = 0
foreach ($c in $Caminhos) {
    $alvo = Join-Path $Repo $c
    if (-not (Test-Path -LiteralPath $alvo)) { $naoAchei += $c; continue }
    $itens = if (Test-Path -LiteralPath $alvo -PathType Leaf) { @(Get-Item -LiteralPath $alvo -Force) }
             else { @(Get-ChildItem -LiteralPath $alvo -Recurse -File -Force) }
    foreach ($i in $itens) {
        $rel = $i.FullName.Substring($Repo.Length).TrimStart('\', '/') -replace '\\', '/'
        $partes = $rel -split '/'
        $fora = ($partes | Where-Object { $pastasFora -contains $_ }).Count -gt 0 -or
                ($arquivosFora -contains $i.Name) -or ($extFora -contains $i.Extension.ToLower())
        if ($fora) { $deFora++ } else { [void]$escolhidos.Add($rel) }
    }
}
if ($naoAchei.Count -gt 0) { Write-Host "ERRO: nao achei no repositorio: $($naoAchei -join ', ')"; exit 1 }
if ($escolhidos.Count -eq 0) { Write-Host 'ERRO: nenhum arquivo para zipar.'; exit 1 }

$base = 'kryptos-foto-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
$zip = Join-Path $Amb.downloads ($base + '.zip')
$n = 2
while (Test-Path -LiteralPath $zip) { $zip = Join-Path $Amb.downloads ("$base-$n.zip"); $n++ }

$arquivoZip = [System.IO.Compression.ZipFile]::Open($zip, 'Create')
try {
    foreach ($rel in $escolhidos) {
        $origem = Join-Path $Repo ($rel -replace '/', [System.IO.Path]::DirectorySeparatorChar)
        [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($arquivoZip, $origem, $rel, 'Optimal')
    }
} finally { $arquivoZip.Dispose() }

# Conferencia: le o zip de volta.
$lido = [System.IO.Compression.ZipFile]::OpenRead($zip)
try { $nomesLidos = @($lido.Entries | ForEach-Object { $_.FullName }) } finally { $lido.Dispose() }
if ($nomesLidos.Count -ne $escolhidos.Count) {
    Write-Host "ERRO: zip tem $($nomesLidos.Count) arquivos, esperado $($escolhidos.Count)"; exit 1
}

$kb = [math]::Round((Get-Item -LiteralPath $zip).Length / 1KB, 1)
$h = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.Substring(0, 12)
Write-Host "foto: $zip"
Write-Host "arquivos: $($escolhidos.Count)   deixados de fora: $deFora   tamanho: $kb KB   sha: $h"
Write-Host 'RESULTADO: OK'
exit 0
