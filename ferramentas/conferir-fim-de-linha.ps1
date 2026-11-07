# ferramentas/conferir-fim-de-linha.ps1
# Confere, sem alterar nada, se todo arquivo rastreado pelo git esta com o fim de
# linha que o .gitattributes manda:
#   - no repositorio (indice): texto sempre com LF;
#   - na pasta: CRLF para .ps1/.bat/.cmd, LF para o resto; binario nao e conferido.
# Usa o proprio git como fonte (git ls-files --eol), nao uma lista nossa.
#
# Uso (uma linha; a suite tambem chama):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\conferir-fim-de-linha.ps1

[CmdletBinding()]
param([string]$Raiz)

$ErrorActionPreference = 'Stop'
if (-not $Raiz) { $Raiz = Split-Path -Parent $PSScriptRoot }

function RodarGit([string[]]$Argumentos) {
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & git -C $Raiz -c core.quotepath=off @Argumentos 2>&1 } finally { $ErrorActionPreference = $antes }
    $codigo = $LASTEXITCODE
    $texto = @($saida | ForEach-Object { "$_" })
    if ($codigo -ne 0) { throw "git $($Argumentos -join ' ') saiu com codigo $codigo. $($texto -join ' ')" }
    return $texto
}

$problemas = New-Object System.Collections.Generic.List[string]
$ok = $false
try {
    if (-not (Test-Path -LiteralPath (Join-Path $Raiz '.gitattributes'))) { throw 'falta o .gitattributes na raiz do repositorio' }

    $texto = 0; $binarios = 0
    foreach ($l in (RodarGit @('ls-files', '--eol'))) {
        if ($l -notmatch '^i/(\S*)\s+w/(\S*)\s+attr/(.*?)\s*\t(.+)$') { throw "linha inesperada do git ls-files --eol: $l" }
        $i = $Matches[1]; $w = $Matches[2]; $attr = $Matches[3]; $arq = $Matches[4]
        if ($w -eq '') { continue }                              # apagado na pasta: o git status mostra
        if ($i -eq '-text' -or $attr -match '(^|\s)-text') { $binarios++; continue }
        if ($i -eq 'crlf' -or $i -eq 'mixed') { $problemas.Add("$arq : guardado no repositorio com $i (deveria ser lf)"); continue }
        if ($attr -match 'eol=crlf') { $quer = 'crlf' }
        elseif ($attr -match 'eol=lf') { $quer = 'lf' }
        else { $problemas.Add("$arq : sem regra de fim de linha no .gitattributes (attr '$attr')"); continue }
        $texto++
        if ($w -ne $quer -and $w -ne 'none') { $problemas.Add("$arq : na pasta esta $w, deveria ser $quer") }
    }
    if ($texto -eq 0) { throw 'nenhum arquivo de texto conferido: o git ls-files --eol veio vazio?' }
    if ($problemas.Count -eq 0) { Write-Host "OK    fim de linha: $texto arquivos de texto conferidos, $binarios binarios ignorados"; $ok = $true }
} catch {
    $problemas.Add("FALHA $($_.Exception.Message)")
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host "FALHA fim de linha em $($problemas.Count) arquivo(s):"
foreach ($p in ($problemas | Select-Object -First 25)) { Write-Host "  $p" }
if ($problemas.Count -gt 25) { Write-Host "  ... e mais $($problemas.Count - 25)" }
Write-Host 'RESULTADO: FALHOU'
exit 1
