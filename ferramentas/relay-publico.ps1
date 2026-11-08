# ferramentas/relay-publico.ps1
# Poe o relay do Kryptos na internet a partir deste PC, pelo Tailscale Funnel, para o
# teste com os celulares fora do cabo (4G ou outra rede). Fica no ar ate voce digitar FIM.
#
# Uso (uma linha, numa janela propria; o instalar-no-celular.ps1 roda em outra):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\relay-publico.ps1
#
# Antes da primeira vez: conta no Tailscale, Tailscale instalado neste PC e conectado.
# Na primeira vez o Tailscale pede para aprovar o Funnel: o link aparece nesta janela.
#
# O que faz:
#   1. confere o Tailscale (versao 1.52 ou mais nova, conectado, nome *.ts.net);
#   2. confere que a porta 443 do Funnel nao esta servindo outra coisa;
#   3. compila o relay (release) e sobe so em 127.0.0.1, com o malote em arquivo;
#   4. liga o Funnel na 443 apontando para o relay e le a configuracao de volta;
#   5. confere o /saude por dentro (pela tailnet) e por fora (DNS publico, pelo
#      servidor do Funnel, como o celular no 4G vai fazer);
#   6. grava o endereco num arquivo que o instalar-no-celular.ps1 -RelayPublico le;
#   7. espera FIM. No fim desliga o Funnel (e le de volta), apaga o arquivo do endereco,
#      desliga o relay e mostra as ultimas linhas do log dele (o relay nao grava chave,
#      dono nem conteudo: regra 8.2).
#
# O malote (as cartas ainda nao buscadas, validade de 7 dias) fica em arquivo e
# sobrevive entre uma vez e outra: %LOCALAPPDATA%\Kryptos\relay-publico\malote.redb.
# O estoque de chaves avulsas fica so na memoria; o app repoe quando o relay volta.
#
# Feche com FIM, nao pelo X da janela: pelo X o Funnel continua ligado apontando para
# um relay desligado (quem acessar recebe erro). Se acontecer, rode este script de
# novo (ele reaproveita a configuracao) ou: tailscale funnel --https=443 off

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot

# Porta local do relay. Diferente da 18080 do relay de teste da suite, para os dois
# poderem rodar juntos. Estimo que esteja livre; se nao estiver, o relay para ao subir
# e o script diz.
$Porta = 18090
$Alvo = "http://127.0.0.1:$Porta"
$Pasta = Join-Path (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Kryptos') 'relay-publico'
$ArqEndereco = Join-Path $Pasta 'endereco.txt'
$ArqMalote = Join-Path $Pasta 'malote.redb'

$resultados = New-Object System.Collections.Generic.List[string]
$procRelay = $null
$logsRelay = @()
$funnelLigado = $false
$ts = $null
# Nome do PC na tailnet. Sempre como $script:NomeDoPc: o Passo tem um parametro $Nome, e
# variavel do PowerShell nao diferencia maiuscula; por escopo dinamico, "$nome" dentro
# de um passo era o nome do passo (achado na calibracao de 08/11/2026).
$NomeDoPc = $null

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
    $detalhe = & $Acao
    if ($detalhe) { $resultados.Add("OK    $Nome  ($detalhe)") } else { $resultados.Add("OK    $Nome") }
}

function NomeDoCurl {
    # No Windows PowerShell 5.1, "curl" e apelido do Invoke-WebRequest: o curl de verdade
    # e o curl.exe (vem com o Windows 10 e 11).
    if ($env:OS -eq 'Windows_NT') { return 'curl.exe' }
    return 'curl'
}

# Codigo HTTP do GET, ou 0 quando nem chegou resposta. O motivo da falha volta em $Erro.
function Saude([string]$Url, [string[]]$Extra, [ref]$Erro) {
    # O caminho vai numa variavel: dentro de @(...), a virgula vale antes do +, e
    # "'-o', X + 'y'" virava dois argumentos (achado na calibracao de 08/11/2026).
    $arq = Join-Path ([System.IO.Path]::GetTempPath()) 'kryptos-saude.txt'
    $argumentos = @($Extra) + @('-sS', '-o', $arq, '-w', '%{http_code}', '--max-time', '20', $Url)
    $antes = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $saida = & (NomeDoCurl) @argumentos 2>&1
    } finally { $ErrorActionPreference = $antes }
    $texto = (@($saida | ForEach-Object { "$_" }) -join ' ').Trim()
    Remove-Item -LiteralPath $arq -Force -ErrorAction SilentlyContinue
    if ($texto -match '(\d{3})$') {
        $Erro.Value = $texto
        return [int]$Matches[1]
    }
    $Erro.Value = $texto
    return 0
}

# Configuracao do Funnel/Serve deste PC, ja como objeto (vazia quando nao ha nada).
function LerFunnel {
    $t = Rodar $ts @('funnel', 'status', '--json')
    if (-not $t -or $t -notmatch '^\s*\{') { return [pscustomobject]@{} }
    return ($t | ConvertFrom-Json)
}

function Campo($Objeto, [string]$Nome) {
    # Pelo nome exato, sem o indexador Properties[...]: com a chave "/" ele nao acha
    # nada (achado na calibracao de 08/11/2026).
    if ($null -eq $Objeto) { return $null }
    foreach ($p in $Objeto.PSObject.Properties) { if ($p.Name -ceq $Nome) { return $p.Value } }
    return $null
}

# O que o Funnel serve na 443 deste nome: Proxy do "/" e se esta aberto para a internet.
function Funnel443($Cfg) {
    $hp = "$($script:NomeDoPc):443"
    $web = Campo (Campo $Cfg 'Web') $hp
    $raiz = Campo (Campo $web 'Handlers') '/'
    [pscustomobject]@{
        # Sem a barra final: versoes do tailscale podem gravar com ou sem ela.
        Proxy   = "$(Campo $raiz 'Proxy')".TrimEnd('/')
        Publico = [bool](Campo (Campo $Cfg 'AllowFunnel') $hp)
        Algo    = ($null -ne $web)
    }
}

$ok = $false
try {
    # ---------- 1. Tailscale ----------
    Passo 'Tailscale instalado' {
        $c = Get-Command tailscale -ErrorAction SilentlyContinue
        if ($c) { $script:ts = $c.Source }
        elseif (Test-Path -LiteralPath 'C:\Program Files\Tailscale\tailscale.exe') { $script:ts = 'C:\Program Files\Tailscale\tailscale.exe' }
        else { throw 'nao achei o tailscale. Instale de tailscale.com/download, entre na conta e rode de novo' }
        $v = ((Rodar $ts @('version')) -split "`n")[0].Trim()
        if ($v -notmatch '^(\d+)\.(\d+)') { throw "nao entendi a versao do tailscale: '$v'" }
        # A sintaxe do "tailscale funnel" usada aqui e a da 1.52 em diante.
        if ([int]$Matches[1] -lt 1 -or ([int]$Matches[1] -eq 1 -and [int]$Matches[2] -lt 52)) {
            throw "tailscale $v e antigo; este script precisa da 1.52 ou mais nova (atualize pelo tailscale.com/download)"
        }
        "$v em $ts"
    }

    Passo 'Tailscale conectado' {
        $st = Rodar $ts @('status', '--json') | ConvertFrom-Json
        if ($st.BackendState -ne 'Running') {
            throw "o Tailscale nao esta conectado (estado: $($st.BackendState)). Abra o Tailscale neste PC, entre na conta e rode de novo"
        }
        $dns = "$($st.Self.DNSName)".TrimEnd('.').ToLowerInvariant()
        if (-not $dns -or $dns -notmatch '^[a-z0-9-]+\.[a-z0-9-]+\.ts\.net$') {
            throw "nome deste PC na tailnet inesperado: '$dns' (o Funnel so usa nomes *.ts.net; o MagicDNS esta ligado?)"
        }
        $script:NomeDoPc = $dns
        $dns
    }

    # ---------- 2. a 443 nao pode estar servindo outra coisa ----------
    Passo 'porta 443 do Funnel livre' {
        $f = Funnel443 (LerFunnel)
        if ($f.Algo -and $f.Proxy -ne $Alvo) {
            $o = if ($f.Proxy) { $f.Proxy } else { 'outra configuracao' }
            throw "a 443 deste PC ja serve $o. Nao mexi nela. Para liberar: tailscale funnel --https=443 off"
        }
        if ($f.Algo) { 'ja apontava para o relay (sobra de uma vez anterior); sera reaproveitada' } else { 'livre' }
    }

    # ---------- 3. relay ----------
    Passo 'compilar o relay (release)' {
        Set-Location -LiteralPath $Repo
        Write-Host '   a primeira vez demora alguns minutos'
        Rodar 'cargo' @('build', '--release', '-p', 'kryptos_relay') | Out-Null
        'target\release'
    }

    Passo 'relay neste PC' {
        $nomeExe = if ($env:OS -eq 'Windows_NT') { 'kryptos_relay.exe' } else { 'kryptos_relay' }
        $exe = Join-Path (Join-Path (Join-Path $Repo 'target') 'release') $nomeExe
        if (-not (Test-Path -LiteralPath $exe)) { throw "nao achei $exe depois de compilar" }
        New-Item -ItemType Directory -Path $Pasta -Force | Out-Null
        $base = Join-Path $Pasta ('relay-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
        $script:logsRelay = @("$base.log", "$base.err")
        $env:KRYPTOS_RELAY_ENDERECO = "127.0.0.1:$Porta"
        $env:KRYPTOS_RELAY_MALOTE = $ArqMalote
        # Log sem codigos de cor (o tracing respeita NO_COLOR): ele e lido e mostrado aqui.
        $env:NO_COLOR = '1'
        try {
            $script:procRelay = Start-Process -FilePath $exe -PassThru -NoNewWindow `
                -RedirectStandardOutput $script:logsRelay[0] -RedirectStandardError $script:logsRelay[1]
        } finally {
            Remove-Item Env:\KRYPTOS_RELAY_ENDERECO -ErrorAction SilentlyContinue
            Remove-Item Env:\KRYPTOS_RELAY_MALOTE -ErrorAction SilentlyContinue
            Remove-Item Env:\NO_COLOR -ErrorAction SilentlyContinue
        }
        # Espera o /saude ate 10 s. Cada tentativa que falha so quer dizer "ainda nao subiu".
        $pronto = $false
        $motivo = ''
        for ($i = 0; $i -lt 20 -and -not $pronto; $i++) {
            if ($script:procRelay.HasExited) {
                $err = ''
                foreach ($l in $script:logsRelay) { if (Test-Path -LiteralPath $l) { $err += (Get-Content -LiteralPath $l -Raw) } }
                throw "o relay parou ao subir (porta $Porta ocupada?). Log:`n$err"
            }
            if ((Saude "$Alvo/saude" @() ([ref]$motivo)) -eq 200) { $pronto = $true } else { Start-Sleep -Milliseconds 500 }
        }
        if (-not $pronto) { throw "o relay nao respondeu em 10 s na porta $Porta ($motivo)" }
        # Prova de que o malote e o de arquivo: o relay diz no log e o arquivo existe.
        $log = ''
        foreach ($l in $script:logsRelay) { if (Test-Path -LiteralPath $l) { $log += (Get-Content -LiteralPath $l -Raw) } }
        if ($log -notmatch 'malote em arquivo') { throw "o relay subiu sem o malote em arquivo. Log:`n$log" }
        if (-not (Test-Path -LiteralPath $ArqMalote)) { throw "o relay diz malote em arquivo, mas $ArqMalote nao existe" }
        "127.0.0.1:$Porta, malote em $ArqMalote ($([math]::Round((Get-Item -LiteralPath $ArqMalote).Length / 1KB)) KB)"
    }

    # ---------- 4. Funnel ----------
    Passo 'Funnel ligado na 443' {
        Write-Host '   se o Tailscale mostrar um link abaixo, abra no navegador e aprove o Funnel'
        # Sem capturar a saida: na primeira vez o tailscale mostra o link de aprovacao e
        # espera. Codigo 0 nao prova nada (quando nao espera, ele sai com 0 sem ligar):
        # a prova e a configuracao lida de volta logo abaixo.
        $antes = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        # Out-Host: sem ele a saida virava o detalhe do passo, e o link nao aparecia.
        try { & $ts funnel --bg --https=443 $Alvo 2>&1 | Out-Host } finally { $ErrorActionPreference = $antes }
        $codigo = $LASTEXITCODE
        $script:funnelLigado = $true
        if ($codigo -ne 0) { throw "tailscale funnel saiu com codigo $codigo (mensagem acima)" }
        $f = Funnel443 (LerFunnel)
        if ($f.Proxy -ne $Alvo) {
            throw "lido de volta, a 443 nao aponta para o relay (aponta para: '$($f.Proxy)'). Se apareceu um link acima, aprove o Funnel nele e rode de novo"
        }
        if (-not $f.Publico) {
            throw 'lido de volta, a 443 serve o relay so dentro da tailnet, sem Funnel. Se apareceu um link acima, aprove o Funnel nele e rode de novo'
        }
        "https://$script:NomeDoPc -> $Alvo, aberto para a internet"
    }

    $endereco = "https://$script:NomeDoPc"

    # ---------- 5. por dentro e por fora ----------
    Passo 'saude por dentro (pela tailnet)' {
        # Daqui o nome resolve para este proprio PC (MagicDNS): prova o certificado e o
        # repasse do Tailscale para o relay. Na primeira vez o certificado e emitido na
        # hora e pode levar alguns segundos.
        $motivo = ''
        $cod = 0
        for ($i = 0; $i -lt 6 -and $cod -ne 200; $i++) {
            $cod = Saude "$endereco/saude" @() ([ref]$motivo)
            if ($cod -ne 200) { Start-Sleep -Seconds 5 }
        }
        if ($cod -ne 200) { throw "https://$script:NomeDoPc/saude nao respondeu 200 por dentro: $motivo" }
        'HTTP 200, certificado valido'
    }

    $porFora = $false
    if (-not (Get-Command Resolve-DnsName -ErrorAction SilentlyContinue)) {
        $resultados.Add('AVISO saude por fora nao conferida: falta o Resolve-DnsName (so existe no Windows)')
    } else {
        Write-Host '.. saude por fora (DNS publico, pelo servidor do Funnel)'
        Write-Host '   o DNS publico do nome pode levar ate 10 min na primeira vez; confiro a cada 20 s'
        $motivo = ''
        $ip = $null
        $limite = (Get-Date).AddMinutes(10)
        while (-not $porFora -and (Get-Date) -lt $limite) {
            try {
                $ip = @(Resolve-DnsName -Name $script:NomeDoPc -Type A -Server 1.1.1.1 -DnsOnly -ErrorAction Stop |
                    Where-Object { $_.Type -eq 'A' } | ForEach-Object { $_.IPAddress })[0]
            } catch {
                $ip = $null
                $motivo = "DNS publico ainda sem o nome ($($_.Exception.Message))"
            }
            if ($ip) {
                # --resolve: vai ao IP do servidor do Funnel achado no DNS publico, sem
                # passar pelo MagicDNS deste PC. E o caminho do celular no 4G.
                $cod = Saude "$endereco/saude" @('--resolve', "$($script:NomeDoPc):443:$ip") ([ref]$motivo)
                if ($cod -eq 200) { $porFora = $true; break }
            }
            Write-Host "   ainda nao ($motivo)"
            Start-Sleep -Seconds 20
        }
        if ($porFora) { $resultados.Add("OK    saude por fora (DNS publico, pelo servidor do Funnel)  (HTTP 200 via $ip)") }
        else { $resultados.Add("AVISO saude por fora nao respondeu em 10 min ($motivo). O relay fica no ar; o celular tenta de novo a cada 10 s") }
    }

    # ---------- 6. endereco para o instalar-no-celular.ps1 ----------
    Passo 'endereco gravado para o instalar-no-celular.ps1' {
        [System.IO.File]::WriteAllText($ArqEndereco, $endereco, (New-Object System.Text.UTF8Encoding($false)))
        $lido = [System.IO.File]::ReadAllText($ArqEndereco).Trim()
        if ($lido -ne $endereco) { throw "lido de volta de $ArqEndereco : '$lido', esperado '$endereco'" }
        $ArqEndereco
    }

    # ---------- 7. no ar ate FIM ----------
    Write-Host ''
    foreach ($linha in $resultados) { Write-Host $linha }
    Write-Host ''
    Write-Host "RELAY NA INTERNET: $endereco"
    Write-Host 'Em outra janela:'
    Write-Host '  powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -RelayPublico -TelaLivre'
    Write-Host 'Deixe esta janela aberta durante o teste e digite FIM aqui quando terminar.'
    try { $Host.UI.RawUI.FlushInputBuffer() } catch { Write-Host "   aviso: nao consegui limpar o buffer do teclado ($($_.Exception.GetType().Name)); so FIM encerra" }
    $inicio = Get-Date
    do {
        $resposta = Read-Host 'Digite FIM para encerrar'
        if ($procRelay.HasExited) { throw "o relay parou sozinho durante o teste (codigo $($procRelay.ExitCode))" }
    } while ($resposta.Trim().ToUpperInvariant() -ne 'FIM')
    $resultados.Add("OK    relay no ar por $([math]::Round(((Get-Date) - $inicio).TotalMinutes, 1)) min")

    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    if (Test-Path -LiteralPath $ArqEndereco) {
        Remove-Item -LiteralPath $ArqEndereco -Force
        Write-Host 'endereco apagado (o instalar-no-celular.ps1 -RelayPublico nao acha relay no ar)'
    }
    if ($funnelLigado) {
        $antes = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        $saidaOff = & $ts funnel --https=443 off 2>&1
        $ErrorActionPreference = $antes
        try {
            $f = Funnel443 (LerFunnel)
            if ($f.Algo) { $resultados.Add("FALHA o Funnel continua ligado na 443 ($($f.Proxy)): rode tailscale funnel --https=443 off. Saida: $saidaOff"); $ok = $false }
            else { $resultados.Add('OK    Funnel desligado (lido de volta: nada na 443)') }
        } catch {
            $resultados.Add("FALHA nao consegui conferir se o Funnel desligou: $($_.Exception.Message)"); $ok = $false
        }
    }
    if ($procRelay -and -not $procRelay.HasExited) {
        Stop-Process -Id $procRelay.Id -Force
        $procRelay.WaitForExit(5000) | Out-Null
        $resultados.Add('OK    relay desligado')
    }
    $linhasLog = @()
    foreach ($l in $logsRelay) {
        if (Test-Path -LiteralPath $l) {
            $linhasLog += @(Get-Content -LiteralPath $l | Where-Object { $_ -match '\S' })
            Remove-Item -LiteralPath $l -Force -ErrorAction Continue
        }
    }
    if ($linhasLog.Count -gt 0) {
        $resultados.Add("..    log do relay: $($linhasLog.Count) linhas (ultimas 20)")
        foreach ($l in ($linhasLog | Select-Object -Last 20)) { $resultados.Add("      $($l.Trim())") }
    }
    Set-Location -LiteralPath $Repo
    Write-Host ''
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
