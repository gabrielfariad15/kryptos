# kryptos-criar-estrutura.ps1
# Clona o repositorio em C:\kryptos, cria as pastas (backend, frontend, nucleo,
# documentacao, ferramentas) e copia a documentacao que esta em Downloads.
# Nao faz commit. Pode rodar de novo: o que ja existe e mantido.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File "$env:USERPROFILE\Downloads\kryptos-criar-estrutura.ps1"

[CmdletBinding()]
param(
    [string]$Raiz = 'C:\kryptos',
    [string]$Repo = 'https://github.com/gabrielfariad15/kryptos.git',
    [string]$Downloads = (Join-Path $env:USERPROFILE 'Downloads')
)

$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
$sep = [System.IO.Path]::DirectorySeparatorChar
function Caminho([string]$relativo) { Join-Path $Raiz ($relativo.Replace('/', $sep)) }

function Git-Rodar {
    # git escreve avisos no stderr; no Windows PowerShell 5.1 isso viraria erro fatal.
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $saida = & git @args 2>&1 } finally { $ErrorActionPreference = $antes }
    return @{ Codigo = $LASTEXITCODE; Saida = ($saida | Out-String).Trim() }
}

function Escrever-SeNaoExiste([string]$relativo, [string]$texto) {
    $destino = Caminho $relativo
    if (Test-Path -LiteralPath $destino) { Write-Host "  mantido (ja existia): $relativo"; return }
    [System.IO.File]::WriteAllText($destino, $texto, $utf8)
    Write-Host "  criado: $relativo"
}

Write-Host '== 1. Repositorio'
if (-not (Get-Command git -ErrorAction SilentlyContinue)) { Write-Host 'ERRO: git nao encontrado no PATH.'; exit 1 }

if (Test-Path -LiteralPath $Raiz) {
    if (-not (Test-Path -LiteralPath (Join-Path $Raiz '.git'))) {
        Write-Host "ERRO: $Raiz ja existe e nao e repositorio git. Nada foi alterado."; exit 1
    }
    $r = Git-Rodar -C $Raiz remote get-url origin
    if ($r.Codigo -ne 0 -or $r.Saida -ne $Repo) {
        Write-Host "ERRO: $Raiz aponta para '$($r.Saida)', nao para $Repo. Nada foi alterado."; exit 1
    }
    Write-Host "  repositorio ja existe em $Raiz"
} else {
    $r = Git-Rodar clone $Repo $Raiz
    if ($r.Codigo -ne 0) { Write-Host "ERRO: git clone falhou:`n$($r.Saida)"; exit 1 }
    Write-Host "  clonado em $Raiz"
}

Write-Host '== 2. Pastas'
$pastas = @(
    'documentacao', 'documentacao/arquitetura', 'documentacao/telas',
    'ferramentas',
    'backend', 'backend/relay', 'backend/atestacao', 'backend/infra',
    'frontend', 'frontend/app', 'frontend/design',
    'nucleo'
)
foreach ($p in $pastas) {
    $d = Caminho $p
    if (Test-Path -LiteralPath $d) { Write-Host "  mantida: $p" }
    else { New-Item -ItemType Directory -Path $d | Out-Null; Write-Host "  criada: $p" }
}

Write-Host '== 3. Arquivos de base'
$readme = @'
# Kryptos

Mensageiro privado da Torion Systems: criptografia de ponta a ponta, mensagens que queimam depois de lidas e contato por código, sem número de celular.

## Pastas

- `documentacao/` — decisões, arquitetura e telas aprovadas
- `ferramentas/` — scripts do projeto, rodados por uma linha; `ambiente.json` guarda os fatos fixos da máquina
- `backend/` — o que roda no servidor
  - `relay/` — servidor de repasse (Rust): guarda a mensagem cifrada até a entrega e apaga
  - `atestacao/` — verificação da atestação por hardware dos aparelhos (Rust)
  - `infra/` — implantação do servidor
- `frontend/` — o que o usuário vê
  - `app/` — aplicativo Flutter (Android primeiro)
  - `design/` — paleta, fontes e referências das telas
- `nucleo/` — motor do aplicativo em Rust: criptografia, protocolo e queima. Roda no aparelho, não no servidor. O `frontend/app` chama o núcleo pelo flutter_rust_bridge.

## Fase atual

Fase 1: Android, conversa entre duas pessoas. Ver `documentacao/DECISOES-FASE-0.md`.
'@
Escrever-SeNaoExiste 'README.md' $readme

$gitignore = @'
# Rust
target/

# Flutter / Dart
.dart_tool/
.flutter-plugins
.flutter-plugins-dependencies
build/
*.iml

# Android: chaves de assinatura e caminhos locais
local.properties
key.properties
*.jks
*.keystore

# Segredos (o certificado raiz PUBLICO da atestacao pode ir para o repositorio)
.env
segredos/

# Sistema
Thumbs.db
desktop.ini
.DS_Store
'@
Escrever-SeNaoExiste '.gitignore' $gitignore

$ambiente = [ordered]@{
    raiz         = $Raiz
    repositorio  = $Repo
    downloads    = $Downloads
    criado_em    = (Get-Date -Format 'yyyy-MM-dd HH:mm')
}
Escrever-SeNaoExiste 'ferramentas/ambiente.json' (($ambiente | ConvertTo-Json) + "`n")

Write-Host '== 4. Documentacao vinda de Downloads'
$pendentes = @()
$origemDec = Join-Path $Downloads 'KRYPTOS-decisoes-2026-11-06.md'
$destinoDec = Caminho 'documentacao/DECISOES-FASE-0.md'
if (Test-Path -LiteralPath $destinoDec) {
    Write-Host '  mantido (ja existia): documentacao/DECISOES-FASE-0.md'
} elseif (Test-Path -LiteralPath $origemDec) {
    Copy-Item -LiteralPath $origemDec -Destination $destinoDec
    $h1 = (Get-FileHash -LiteralPath $origemDec -Algorithm SHA256).Hash
    $h2 = (Get-FileHash -LiteralPath $destinoDec -Algorithm SHA256).Hash
    if ($h1 -ne $h2) { Write-Host '  ERRO: copia de DECISOES-FASE-0.md difere da origem.'; exit 1 }
    Write-Host "  copiado e conferido (sha256 $($h2.Substring(0,12))): documentacao/DECISOES-FASE-0.md"
} else {
    Write-Host "  AVISO: nao achei $origemDec"
    $pendentes += 'documentacao/DECISOES-FASE-0.md'
}

$destinoScript = Caminho 'ferramentas/criar-estrutura.ps1'
if (Test-Path -LiteralPath $destinoScript) {
    Write-Host '  mantido (ja existia): ferramentas/criar-estrutura.ps1'
} else {
    Copy-Item -LiteralPath $PSCommandPath -Destination $destinoScript
    Write-Host '  copiado: ferramentas/criar-estrutura.ps1'
}

# Git nao guarda pasta vazia: .gitkeep so onde nao ha nada.
foreach ($p in $pastas) {
    $d = Caminho $p
    if (-not (Get-ChildItem -LiteralPath $d -Force | Select-Object -First 1)) {
        [System.IO.File]::WriteAllText((Join-Path $d '.gitkeep'), '', $utf8)
    }
}

Write-Host '== 5. Conferencia (lendo de volta do disco)'
$esperados = $pastas + @('README.md', '.gitignore', 'ferramentas/ambiente.json', 'ferramentas/criar-estrutura.ps1')
if ($pendentes.Count -eq 0) { $esperados += 'documentacao/DECISOES-FASE-0.md' }
$faltando = @($esperados | Where-Object { -not (Test-Path -LiteralPath (Caminho $_)) })
$readmeLido = [System.IO.File]::ReadAllText((Caminho 'README.md'), $utf8)
if ($readmeLido -notmatch 'núcleo') { $faltando += 'README.md (acentos corrompidos)' }

Write-Host "  esperados: $($esperados.Count)  presentes: $($esperados.Count - $faltando.Count)  faltando: $($faltando.Count)"
foreach ($f in $faltando) { Write-Host "  FALTA: $f" }
foreach ($p in $pendentes) { Write-Host "  PENDENTE (copiar a mao): $p" }

Write-Host '== 6. git status'
$st = Git-Rodar -C $Raiz status --short --untracked-files=all
Write-Host $st.Saida

if ($faltando.Count -gt 0) { Write-Host 'RESULTADO: FALHOU'; exit 1 }
Write-Host 'RESULTADO: OK'
exit 0
