# ferramentas/conferir-rust.ps1
# Confere, sem alterar nada, que o Rust da suite e o Rust do celular sao a mesma
# versao: a do rust-toolchain.toml.
#   - suite, relay e editor: o cargo usa a versao do rust-toolchain.toml;
#   - app no celular: o Cargokit compila com a "stable" do rustup e nao le o
#     rust-toolchain.toml (so aceita stable, beta ou nightly).
# Se a stable andar (rustup update) e o arquivo nao, o celular passaria a compilar
# com outro Rust sem ninguem ver. Aqui isso vira FALHA, e trocar de versao vira
# decisao: subir o rust-toolchain.toml com a suite verde na versao nova.
#
# Uso (uma linha; a suite e o instalar-no-celular tambem chamam):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\conferir-rust.ps1

[CmdletBinding()]
param([string]$Raiz)

$ErrorActionPreference = 'Stop'
if (-not $Raiz) { $Raiz = Split-Path -Parent $PSScriptRoot }

function Rodar([string]$Exe, [string[]]$Argumentos) {
    # Comando externo. Avisos no stderr nao derrubam; codigo de saida != 0 derruba.
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & $Exe @Argumentos 2>&1 } finally { $ErrorActionPreference = $antes }
    $codigo = $LASTEXITCODE
    $texto = (@($saida | ForEach-Object { "$_" }) -join "`n").Trim()
    if ($codigo -ne 0) { throw "$Exe $($Argumentos -join ' ') saiu com codigo $codigo. $texto" }
    return $texto
}

function Versao([string]$Texto, [string]$Programa) {
    # "rustc 1.98.0 (2d8144b78 2026-07-07)" -> "1.98.0". Procura em todas as linhas:
    # na primeira vez o rustup instala o que o rust-toolchain.toml pede e escreve
    # "info: ..." antes da versao.
    foreach ($linha in ($Texto -split "`n")) {
        if ($linha.Trim() -match "^$Programa (\d+\.\d+\.\d+)(\s|-|$)") { return $Matches[1] }
    }
    throw "nao entendi a versao em: $Texto"
}

$resultados = New-Object System.Collections.Generic.List[string]
$ok = $false
$local = Get-Location
try {
    $arq = Join-Path $Raiz 'rust-toolchain.toml'
    if (-not (Test-Path -LiteralPath $arq)) { throw 'falta o rust-toolchain.toml na raiz do repositorio' }
    $pino = $null
    foreach ($l in (Get-Content -LiteralPath $arq -Encoding UTF8)) {
        if ($l -match '^\s*channel\s*=\s*"(\d+\.\d+\.\d+)"\s*$') { $pino = $Matches[1] }
    }
    if (-not $pino) { throw 'o rust-toolchain.toml nao tem channel = "X.Y.Z" (versao exata, nao stable)' }
    $resultados.Add("OK    rust-toolchain.toml: $pino")

    # Uma variavel RUSTUP_TOOLCHAIN passa por cima do rust-toolchain.toml.
    if ($env:RUSTUP_TOOLCHAIN) { throw "RUSTUP_TOOLCHAIN=$($env:RUSTUP_TOOLCHAIN) esta definida e passa por cima do rust-toolchain.toml; apague a variavel" }

    $temPino = $true
    try { $vPino = Versao (Rodar 'rustup' @('run', $pino, 'rustc', '--version')) 'rustc' }
    catch { $temPino = $false }
    if (-not $temPino) { throw "o Rust $pino nao esta instalado (rode ferramentas\instalar-ferramentas.ps1)" }
    if ($vPino -ne $pino) { throw "rustup run $pino deu rustc $vPino" }

    # O cargo de dentro do repositorio tem de ser o do rust-toolchain.toml.
    Set-Location -LiteralPath $Raiz
    $vCargo = Versao (Rodar 'cargo' @('--version')) 'cargo'
    if ($vCargo -ne $pino) { throw "o cargo do repositorio e $vCargo, nao ${pino}: o rust-toolchain.toml nao esta valendo (rustup override?)" }
    $resultados.Add("OK    suite: cargo $vCargo")

    $temStable = $true
    try { $vStable = Versao (Rodar 'rustup' @('run', 'stable', 'rustc', '--version')) 'rustc' }
    catch { $temStable = $false }
    if (-not $temStable) { throw 'a stable do rustup nao esta instalada: o Cargokit compila o app com ela (rustup toolchain install stable)' }
    if ($vStable -ne $pino) {
        $comeco = "o celular compilaria com a stable $vStable e a suite com $pino."
        if ([version]$vStable -gt [version]$pino) {
            throw "$comeco A stable andou: suba o rust-toolchain.toml para $vStable e rode a suite (versao nova e decisao), nas duas maquinas"
        }
        throw "$comeco A stable ficou para tras: rode 'rustup update stable' e confira de novo"
    }
    $resultados.Add("OK    celular: stable $vStable")
    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    Set-Location -LiteralPath $local
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host "OK    versao do Rust: $pino (suite e celular)"; Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
