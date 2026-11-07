# ferramentas/normalizar-fim-de-linha.ps1
# Aplica o .gitattributes ao que ja existe. Roda uma vez por repositorio (e de novo
# so se o .gitattributes mudar). Nao faz commit.
#
#   1. git add --renormalize: refaz no indice a conversao de todo arquivo rastreado.
#      Mostra quais arquivos mudaram por causa disso (o esperado aqui e nenhum: o
#      git do Windows ja guardava com LF).
#   2. Na pasta, troca so o fim de linha dos arquivos que estao diferentes do que o
#      .gitattributes manda. O conteudo nao muda (mudanca nao commitada e mantida).
#   3. Le de volta pelo conferir-fim-de-linha.ps1.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\normalizar-fim-de-linha.ps1

[CmdletBinding()]
param([string]$Raiz)

$ErrorActionPreference = 'Stop'
if (-not $Raiz) { $Raiz = Split-Path -Parent $PSScriptRoot }
$sep = [System.IO.Path]::DirectorySeparatorChar
$resultados = New-Object System.Collections.Generic.List[string]

function RodarGit([string[]]$Argumentos) {
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & git -C $Raiz -c core.quotepath=off @Argumentos 2>&1 } finally { $ErrorActionPreference = $antes }
    $codigo = $LASTEXITCODE
    $texto = @($saida | ForEach-Object { "$_" })
    if ($codigo -ne 0) { throw "git $($Argumentos -join ' ') saiu com codigo $codigo. $($texto -join ' ')" }
    return $texto
}

function TrocarFimDeLinha([string]$Caminho, [string]$Quer) {
    # Mexe so nos bytes de fim de linha: CRLF -> LF e, se pedido, LF -> CRLF.
    # CR solto (sem LF depois) fica como esta. BOM e resto do conteudo nao mudam.
    $b = [System.IO.File]::ReadAllBytes($Caminho)
    $saida = New-Object System.IO.MemoryStream
    for ($k = 0; $k -lt $b.Length; $k++) {
        if ($b[$k] -eq 13 -and ($k + 1) -lt $b.Length -and $b[$k + 1] -eq 10) { continue }
        if ($b[$k] -eq 10 -and $Quer -eq 'crlf') { $saida.WriteByte(13) }
        $saida.WriteByte($b[$k])
    }
    # Grava no proprio arquivo (nao em temporario + mover) para manter permissoes, como
    # o bit de execucao no Linux. Se cair no meio, o conteudo continua no git.
    [System.IO.File]::WriteAllBytes($Caminho, $saida.ToArray())
}

$ok = $false
try {
    if (-not (Test-Path -LiteralPath (Join-Path $Raiz '.gitattributes'))) { throw 'falta o .gitattributes na raiz do repositorio' }
    $rastreado = @(RodarGit @('ls-files', '--', '.gitattributes'))
    $noIndice = @(RodarGit @('diff', '--cached', '--name-only', '--', '.gitattributes'))
    if ($rastreado.Count -eq 0 -and $noIndice.Count -eq 0) { throw '.gitattributes ainda fora do git: rode git add .gitattributes antes' }

    # O renormalize e um git add: pegaria junto qualquer mudanca nao preparada.
    # Por isso so roda com a pasta sem mudanca fora do indice.
    $soltas = @(RodarGit @('diff', '--name-only'))
    if ($soltas.Count -gt 0) { throw "ha mudanca fora do indice; faca commit ou git add dela antes (nada foi alterado):`n  $($soltas -join "`n  ")" }

    # ---------- 1. indice ----------
    $antes = @(RodarGit @('diff', '--cached', '--name-only'))
    RodarGit @('add', '--renormalize', '.') | Out-Null
    $depois = @(RodarGit @('diff', '--cached', '--name-only'))
    $soRenormalize = @($depois | Where-Object { $antes -notcontains $_ })
    $resultados.Add("OK    indice: git add --renormalize mudou $($soRenormalize.Count) arquivo(s) alem dos que ja estavam no indice")
    foreach ($a in ($soRenormalize | Select-Object -First 25)) { $resultados.Add("        renormalizado: $a") }

    # ---------- 2. pasta ----------
    $trocados = 0
    $trocadosLista = New-Object System.Collections.Generic.List[string]
    foreach ($l in (RodarGit @('ls-files', '--eol'))) {
        if ($l -notmatch '^i/(\S*)\s+w/(\S*)\s+attr/(.*?)\s*\t(.+)$') { throw "linha inesperada do git ls-files --eol: $l" }
        $i = $Matches[1]; $w = $Matches[2]; $attr = $Matches[3]; $arq = $Matches[4]
        if ($w -eq '' -or $w -eq 'none' -or $i -eq '-text' -or $attr -match '(^|\s)-text') { continue }
        if ($attr -match 'eol=crlf') { $quer = 'crlf' } elseif ($attr -match 'eol=lf') { $quer = 'lf' } else { continue }
        if ($w -eq $quer) { continue }
        TrocarFimDeLinha (Join-Path $Raiz ($arq.Replace('/', $sep))) $quer
        $trocadosLista.Add($arq)
        $trocados++
    }
    # O indice guarda o tamanho de cada arquivo, e o git trata tamanho diferente como
    # "modificado" sem olhar o conteudo; a troca de CRLF/LF muda o tamanho. O git add
    # dos trocados so atualiza esse registro: o conteudo no indice continua o mesmo,
    # o que e conferido logo abaixo (nenhum arquivo novo para o commit).
    $preparadosAntes = @(RodarGit @('diff', '--cached', '--name-only'))
    if ($trocadosLista.Count -gt 0) { RodarGit (@('add', '--') + $trocadosLista) | Out-Null }
    $novosNoCommit = @(RodarGit @('diff', '--cached', '--name-only') | Where-Object { $preparadosAntes -notcontains $_ })
    if ($novosNoCommit.Count -gt 0) { throw "a troca de fim de linha mudou conteudo de: $($novosNoCommit -join ', ')" }
    $aindaModificados = @(RodarGit @('diff', '--name-only') | Where-Object { $trocadosLista -contains $_ })
    if ($aindaModificados.Count -gt 0) { throw "o git ainda ve como modificados: $($aindaModificados -join ', ')" }
    $resultados.Add("OK    pasta: fim de linha trocado em $trocados arquivo(s); conteudo igual ao do git, nenhum entrou no commit")

    # ---------- 3. ler de volta ----------
    $ps = (Get-Process -Id $PID).Path
    $conf = & $ps -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'conferir-fim-de-linha.ps1') -Raiz $Raiz 2>&1
    $codigo = $LASTEXITCODE
    foreach ($c in $conf) { if ("$c" -notmatch '^RESULTADO') { $resultados.Add("$c") } }
    if ($codigo -ne 0) { throw 'a conferencia ainda acha arquivo fora da regra (lista acima)' }

    $total = @(RodarGit @('diff', '--cached', '--name-only')).Count
    $resultados.Add("OK    no indice agora: $total mudanca(s) para o commit")
    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
