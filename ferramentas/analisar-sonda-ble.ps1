# ferramentas/analisar-sonda-ble.ps1
# Resume as medidas da sonda BLE do modo Direto (documentacao/arquitetura/modo-direto.md,
# passo 1). Le os arquivos de medidas, um por celular, que o
# instalar-no-celular.ps1 -SondaBle traz de cada celular depois do FIM, e imprime:
#   - o que cada celular tem de radio (inventario);
#   - anuncios, tempo ate achar, conexao (MTU, pedaco, PHY);
#   - idas e voltas por distancia, PHY, prioridade, caminho da volta (NOTIFICACAO,
#     ESCRITA ou L2CAP) e celular que procura (a volta lenta de 08/11/2026 dependia de
#     quem conecta): conferidas, tempo e kbit/s (mediana, minimo, maximo) e RSSI;
#   - o preparo dos caminhos ESCRITA e L2CAP (quanto levou para abrir);
#   - o lado do servidor, a calibracao, o desfecho do modo AMBOS e as falhas por codigo;
#   - o alcance por PHY: ate onde conferiu e onde deixou de achar.
#
# Uso (uma linha):
#   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\analisar-sonda-ble.ps1 -Arquivos <a.txt>,<b.txt>
#
# RESULTADO: OK so com os arquivos lidos e pelo menos uma calibracao OK e nenhuma
# calibracao FALHOU: sem o conferidor calibrado, as medidas nao valem (regra 5.3). Cada
# caminho de volta medido precisa da sua calibracao OK: a moldura do L2CAP e a escrita do
# servidor sao codigo novo no caminho dos bytes.
# Arquivos de antes do caminho de volta (sem volta=) contam como NOTIFICACAO.
# Rodadas de calibracao (calibrar=sim) ficam fora das estatisticas de ida e volta.
# Distancia com RSSI quase igual ao de 1 m sai como AVISO e fica fora do alcance.
# As linhas tem so codigos e numeros (regra 8.2): nada a esconder ao colar a saida.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string[]]$Arquivos
)

$ErrorActionPreference = 'Stop'
$ci = [Globalization.CultureInfo]::InvariantCulture
$resultados = New-Object System.Collections.Generic.List[string]

function Num([string]$t) {
    $v = 0.0
    if ([double]::TryParse($t, [Globalization.NumberStyles]::Float, $ci, [ref]$v)) { return $v }
    return $null
}

function Mediana($valores) {
    $s = @($valores | Where-Object { $null -ne $_ } | Sort-Object)
    if ($s.Count -eq 0) { return $null }
    $m = [int][math]::Floor($s.Count / 2)
    if ($s.Count % 2 -eq 1) { return [double]$s[$m] }
    return ([double]$s[$m - 1] + [double]$s[$m]) / 2
}

function F($x) {
    if ($null -eq $x) { return '?' }
    return [string]::Format($ci, '{0:0.#}', [double]$x)
}

function Campos([string]$t) {
    $h = @{}
    if (-not $t) { return $h }
    foreach ($p in ($t -split '\s+')) {
        $i = $p.IndexOf('=')
        if ($i -gt 0) { $h[$p.Substring(0, $i)] = $p.Substring($i + 1) }
    }
    return $h
}

# Caminho da volta de uma linha; linhas antigas, sem o campo, eram todas por notificacao.
function Volta($r) {
    if ($r.C.ContainsKey('volta')) { return $r.C['volta'] }
    if ($r.C.ContainsKey('caminho')) { return $r.C['caminho'] }
    return 'NOTIFICACAO'
}

function Valores($lista, [string]$campo) {
    @(foreach ($r in $lista) { if ($r.C.ContainsKey($campo)) { Num $r.C[$campo] } })
}

# Chamado de outro script com -File, a lista chega como um texto so, separado por virgula.
$Arquivos = @($Arquivos | ForEach-Object { $_ -split ',' } | Where-Object { $_.Trim() } | ForEach-Object { $_.Trim() })

$ok = $false
try {
    # ---------- ler ----------
    $regs = New-Object System.Collections.Generic.List[object]
    $malformadas = 0
    foreach ($arq in $Arquivos) {
        if (-not (Test-Path -LiteralPath $arq)) { throw "nao achei $arq" }
        $linhas = @([System.IO.File]::ReadAllLines($arq))
        $aparelho = [System.IO.Path]::GetFileNameWithoutExtension($arq)
        $doArquivo = 0
        foreach ($l in $linhas) {
            if ($l -match '^\s*$') { continue }
            if ($l -notmatch '^(\d\d:\d\d:\d\d\.\d{3}) (MEDIDA|FALHA) sonda-ble (\S+)(?: (.*))?$') { $malformadas++; continue }
            $hora = $Matches[1]; $tipo = $Matches[2]; $evento = $Matches[3]; $c = Campos $Matches[4]
            if ($tipo -eq 'MEDIDA' -and $evento -eq 'aparelho' -and $c.ContainsKey('modelo')) { $aparelho = $c['modelo'] }
            $regs.Add([pscustomobject]@{ Arquivo = $arq; Aparelho = $aparelho; Hora = $hora; Tipo = $tipo; Evento = $evento; C = $c })
            $doArquivo++
        }
        if ($doArquivo -eq 0) { throw "$arq nao tem nenhuma linha da sonda (o app gravou? o run-as leu?)" }
        $resultados.Add("OK    arquivo $([System.IO.Path]::GetFileName($arq)): $doArquivo linhas ($aparelho)")
    }
    if ($malformadas -gt 0) { $resultados.Add("AVISO $malformadas linhas fora do formato foram ignoradas") }

    $medidas = @($regs | Where-Object { $_.Tipo -eq 'MEDIDA' })
    $falhas = @($regs | Where-Object { $_.Tipo -eq 'FALHA' })

    # ---------- aparelhos ----------
    foreach ($r in @($medidas | Where-Object { $_.Evento -eq 'aparelho' } | Sort-Object Aparelho -Unique)) {
        $c = $r.C
        $campos = @('android', 'le2m', 'coded', 'anuncio_estendido', 'anuncio_max_bytes', 'anuncios_multiplos', 'wifi_aware', 'wifi_direct', 'nfc', 'uwb') |
            ForEach-Object { if ($c.ContainsKey($_)) { "$_=$($c[$_])" } }
        $resultados.Add("OK    aparelho $($r.Aparelho): $($campos -join ' ')")
    }

    # ---------- anuncios ----------
    foreach ($g in @($medidas | Where-Object { $_.Evento -eq 'anuncio' } | Group-Object { "$($_.Aparelho) $($_.C['tipo']) $($_.C['resultado']) $($_.C['tx_dbm'])" })) {
        $r = $g.Group[0]
        $tx = if ($r.C.ContainsKey('tx_dbm')) { ", potencia $($r.C['tx_dbm']) dBm" } else { '' }
        $resultados.Add("OK    anuncio $($r.Aparelho) $($r.C['tipo']): $($r.C['resultado'])$tx ($($g.Count)x)")
    }

    # ---------- achou ----------
    foreach ($g in @($medidas | Where-Object { $_.Evento -eq 'achou' } |
            Group-Object { "{0,4} {1}" -f $_.C['distancia'], $_.C['phy'] } | Sort-Object { [int](Num $_.Group[0].C['distancia']) }, { $_.Group[0].C['phy'] })) {
        $r = $g.Group[0]
        $resultados.Add("OK    achou   a $($r.C['distancia']) m, $($r.C['phy']): $($g.Count)x; ms mediana $(F (Mediana (Valores $g.Group 'ms'))); rssi mediana $(F (Mediana (Valores $g.Group 'rssi'))) dBm")
    }

    # ---------- conexao ----------
    foreach ($g in @($medidas | Where-Object { $_.Evento -eq 'conexao' } |
            Group-Object { "$($_.C['distancia']) $($_.C['phy']) $($_.C['prioridade'])" } | Sort-Object { [int](Num $_.Group[0].C['distancia']) }, { $_.Group[0].C['phy'] })) {
        $r = $g.Group[0]
        $mtus = (@($g.Group | ForEach-Object { $_.C['mtu'] } | Sort-Object -Unique)) -join '/'
        $pedacos = (@($g.Group | ForEach-Object { $_.C['pedaco'] } | Sort-Object -Unique)) -join '/'
        $phys = (@($g.Group | ForEach-Object { "$($_.C['phy_tx'])-$($_.C['phy_rx'])" } | Sort-Object -Unique)) -join '/'
        $resultados.Add("OK    conexao a $($r.C['distancia']) m, $($r.C['phy']) $($r.C['prioridade']): $($g.Count)x; ms mediana $(F (Mediana (Valores $g.Group 'ms'))); mtu $mtus; pedaco $pedacos; phy tx-rx $phys")
    }

    # ---------- idas e voltas (fora da calibracao) ----------
    $transf = @($medidas | Where-Object { ($_.Evento -eq 'ida' -or $_.Evento -eq 'volta') -and $_.C['calibrar'] -ne 'sim' })
    # bytes: a carta inteira (66594) e o padrao; menor e o envio do tamanho de mensagem
    # (etapa Mensagem, 10/11/2026), em grupo proprio e com o tamanho escrito na linha.
    foreach ($g in @($transf | Group-Object { "$($_.Evento) $($_.C['distancia']) $($_.C['phy']) $($_.C['prioridade']) $($_.C['bytes']) $(Volta $_) $($_.Aparelho)" } |
            Sort-Object { [int](Num $_.Group[0].C['distancia']) }, { $_.Group[0].C['phy'] }, { $_.Group[0].C['prioridade'] }, { [int](Num $_.Group[0].C['bytes']) }, { Volta $_.Group[0] }, { $_.Group[0].Aparelho }, { $_.Group[0].Evento })) {
        $r = $g.Group[0]
        $conf = @($g.Group | Where-Object { $_.C['resultado'] -eq 'CONFERIU' }).Count
        $msv = Valores $g.Group 'ms'
        $min = ($msv | Measure-Object -Minimum).Minimum
        $max = ($msv | Measure-Object -Maximum).Maximum
        $rssi = if ($r.Evento -eq 'ida') { "; rssi mediana $(F (Mediana (Valores $g.Group 'rssi'))) dBm" } else { '' }
        $marca = if ($conf -eq $g.Count) { 'OK   ' } else { 'AVISO' }
        $tamanho = if ($r.C['bytes'] -and $r.C['bytes'] -ne '66594') { ", $($r.C['bytes']) bytes" } else { '' }
        $resultados.Add("$marca $("{0,-5}" -f $r.Evento) a $($r.C['distancia']) m, $($r.C['phy']) $($r.C['prioridade'])$tamanho, volta $(Volta $r), procura $($r.Aparelho): $conf de $($g.Count) conferidas; ms mediana $(F (Mediana $msv)) (min $(F $min), max $(F $max)); kbit/s mediana $(F (Mediana (Valores $g.Group 'kbps')))$rssi")
    }
    if ($transf.Count -eq 0) { $resultados.Add('AVISO nenhuma ida ou volta fora da calibracao') }

    # ---------- preparo dos caminhos de volta ----------
    # volta-pronta: lado de quem procura (ms desde abrir o proprio servico ou pedir, ate a
    # volta poder comecar). servidor-volta: lado de quem anuncia.
    foreach ($g in @($medidas | Where-Object { $_.Evento -eq 'volta-pronta' -or ($_.Evento -eq 'servidor-volta' -and $_.C['resultado'] -eq 'pronta') } |
            Group-Object { "$($_.Aparelho) $($_.Evento) $(Volta $_)" })) {
        $r = $g.Group[0]
        $extra = @(foreach ($k in @('mtu', 'mtu_origem', 'tx_max', 'rx_max')) {
                $v = (@($g.Group | Where-Object { $_.C.ContainsKey($k) } | ForEach-Object { $_.C[$k] } | Sort-Object -Unique)) -join '/'
                if ($v) { "$k $v" }
            }) -join '; '
        if ($extra) { $extra = "; $extra" }
        $resultados.Add("OK    $($r.Evento) $($r.Aparelho) $(Volta $r): $($g.Count)x; ms mediana $(F (Mediana (Valores $g.Group 'ms')))$extra")
    }

    # ---------- lado do servidor ----------
    foreach ($g in @($medidas | Where-Object { $_.Evento -eq 'servidor-recebeu' -or $_.Evento -eq 'servidor-enviou' } |
            Group-Object { "$($_.Aparelho) $($_.Evento) $(if ($_.Evento -eq 'servidor-enviou') { Volta $_ }) $($_.C['resultado'])" })) {
        $r = $g.Group[0]
        $porOnde = if ($r.Evento -eq 'servidor-enviou') { " por $(Volta $r)" } else { '' }
        $resultados.Add("OK    $($r.Evento) $($r.Aparelho)$($porOnde): $($g.Count)x $($r.C['resultado']); ms mediana $(F (Mediana (Valores $g.Group 'ms'))); mtu $((@($g.Group | ForEach-Object { $_.C['mtu'] } | Sort-Object -Unique)) -join '/')")
    }

    # ---------- AMBOS ----------
    # desfecho=colisao (09/11/2026): os dois conectaram e o OLA desempatou; este= diz o papel que ficou.
    # recuperacao= (09/11/2026): linha cruzada; a sonda fechou e tentou de novo uma vez.
    foreach ($g in @($medidas | Where-Object { $_.Evento -eq 'ambos' -and $_.C.ContainsKey('recuperacao') } | Group-Object { "$($_.Aparelho) $($_.C['recuperacao'])" })) {
        $cods = (@($g.Group | Where-Object { $_.C.ContainsKey('codigo') } | Group-Object { $_.C['codigo'] } | ForEach-Object { "$($_.Name) $($_.Count)" })) -join ', '
        $resultados.Add("OK    ambos $($g.Group[0].Aparelho): recuperacao=$($g.Group[0].C['recuperacao']) $($g.Count)x$(if ($cods) { " ($cods)" })")
    }
    foreach ($g in @($medidas | Where-Object { $_.Evento -eq 'ambos' -and -not $_.C.ContainsKey('recuperacao') } | Group-Object { "$($_.Aparelho) $($_.C['desfecho']) $($_.C['este'])" })) {
        $este = if ($g.Group[0].C.ContainsKey('este')) { " este=$($g.Group[0].C['este'])" } else { '' }
        $resultados.Add("OK    ambos $($g.Group[0].Aparelho): desfecho=$($g.Group[0].C['desfecho'])$este $($g.Count)x")
    }

    # ---------- calibracao ----------
    $cal = @($medidas | Where-Object { $_.Evento -eq 'calibracao' })
    $calOk = @($cal | Where-Object { $_.C['resultado'] -eq 'OK' }).Count
    $calRuim = @($cal | Where-Object { $_.C['resultado'] -ne 'OK' })
    # Na linha da calibracao, volta= e o caminho; ida= e voltas= sao as listas de codigos.
    # Linhas antigas tinham volta= com a lista: caminho NOTIFICACAO.
    function CaminhoCal($r) { if ($r.C['volta'] -match '^(NOTIFICACAO|ESCRITA|L2CAP)$') { return $r.C['volta'] } return 'NOTIFICACAO' }
    function CodigosVolta($r) { if ($r.C.ContainsKey('voltas')) { return $r.C['voltas'] } if ((CaminhoCal $r) -ne $r.C['volta']) { return $r.C['volta'] } return '?' }
    foreach ($r in $calRuim) { $resultados.Add("FALHA calibracao rodada $($r.C['rodada']) ($($r.Aparelho)), volta $(CaminhoCal $r): ida=$($r.C['ida']) voltas=$(CodigosVolta $r)") }
    $caminhosCalibrados = @($cal | Where-Object { $_.C['resultado'] -eq 'OK' } | ForEach-Object { CaminhoCal $_ } | Sort-Object -Unique)
    if ($calOk -gt 0) { $resultados.Add("OK    calibracao: $calOk OK, $($calRuim.Count) falhou; caminhos calibrados: $($caminhosCalibrados -join '/')") }
    $semCal = @($transf | Where-Object { $_.Evento -eq 'volta' } | ForEach-Object { Volta $_ } | Sort-Object -Unique | Where-Object { $caminhosCalibrados -notcontains $_ })

    # ---------- falhas ----------
    foreach ($g in @($falhas | Group-Object Evento | Sort-Object Count -Descending)) {
        # O teste do ciclo de vida do -Autonomo aperta o botao de inicio uma vez de proposito.
        if ($g.Name -eq 'APP_SAIU_DA_FRENTE' -and $g.Count -eq 1) {
            $resultados.Add("OK    app saiu da frente: 1x (esperado no teste do ciclo de vida; mais de uma vez vira AVISO)")
            continue
        }
        $onde = (@($g.Group | Group-Object { if ($_.C.ContainsKey('distancia')) { "$($_.C['phy']) a $($_.C['distancia']) m" } else { $_.Aparelho } } |
                ForEach-Object { "$($_.Name): $($_.Count)" })) -join '; '
        $resultados.Add("AVISO falha $($g.Name): $($g.Count)x ($onde)")
    }

    # ---------- distancia declarada x sinal ----------
    # A DISTANCIA e so uma etiqueta marcada na tela. Em 08/11/2026 as rodadas de 5 a 30 m
    # foram feitas com os celulares juntos, e o RSSI ficou em -40 dBm em todas. Se o sinal
    # numa distancia maior fica a menos de 6 dB do sinal a 1 m (estimo: de 1 para 5 m em
    # espaco aberto cai uns 14 dB; dentro de casa varia uns 5 dB), a etiqueta e suspeita.
    $comRssi = @($medidas | Where-Object { ($_.Evento -eq 'achou' -or $_.Evento -eq 'ida') -and $_.C.ContainsKey('rssi') -and $_.C.ContainsKey('distancia') -and $null -ne (Num $_.C['rssi']) })
    # Cada aparelho le o sinal num nivel proprio (em 08/11 o Edge leu uns 12 dB abaixo do
    # g15 na mesma distancia): a comparacao e de cada aparelho com o 1 m dele mesmo.
    $suspeitas = @()
    foreach ($ap in @($comRssi | Group-Object Aparelho)) {
        $base = Mediana (Valores @($ap.Group | Where-Object { (Num $_.C['distancia']) -eq 1 }) 'rssi')
        $maiores = @($ap.Group | Where-Object { (Num $_.C['distancia']) -gt 1 })
        if ($maiores.Count -eq 0) { continue }
        if ($null -eq $base) {
            $resultados.Add("AVISO $($ap.Name) sem medida a 1 m: nao da para conferir se as distancias maiores foram reais")
            continue
        }
        foreach ($g in @($maiores | Group-Object { [int](Num $_.C['distancia']) } | Sort-Object { [int]$_.Name })) {
            $m = Mediana (Valores $g.Group 'rssi')
            if ($m -ge $base - 6) {
                if ($suspeitas -notcontains [int]$g.Name) { $suspeitas += [int]$g.Name }
                $resultados.Add("AVISO distancia $($g.Name) m ($($ap.Name)): RSSI mediana $(F $m) dBm, quase igual a 1 m ($(F $base) dBm): os celulares estavam mesmo separados? O alcance a $($g.Name) m nao vale")
            }
        }
    }

    # ---------- alcance por PHY ----------
    $ponta = @($medidas | Where-Object { $_.Evento -eq 'ida' -and $_.C['resultado'] -eq 'CONFERIU' })
    $naoAchou = @($falhas | Where-Object { $_.Evento -eq 'NAO_ACHOU_30S' })
    foreach ($phy in @('1M', '2M', 'CODED')) {
        $d = @($ponta | Where-Object { $_.C['phy'] -eq $phy } | ForEach-Object { [int](Num $_.C['distancia']) } | Where-Object { $suspeitas -notcontains $_ })
        $n = @($naoAchou | Where-Object { $_.C['phy'] -eq $phy } | ForEach-Object { [int](Num $_.C['distancia']) } | Where-Object { $suspeitas -notcontains $_ })
        if ($d.Count -eq 0 -and $n.Count -eq 0) { continue }
        $ate = if ($d.Count) { "conferiu ate $(($d | Measure-Object -Maximum).Maximum) m" } else { 'nenhuma ida conferida' }
        $sem = if ($n.Count) { "; nao achou a $((@($n | Sort-Object -Unique)) -join '/') m" } else { '' }
        $resultados.Add("OK    alcance $($phy): $ate$sem")
    }

    if ($calRuim.Count -gt 0) { throw 'a calibracao falhou: o conferidor da sonda nao pegou a sabotagem, as medidas nao valem' }
    if ($calOk -eq 0) { throw 'nenhuma calibracao OK nos arquivos: rode CALIBRAR a 1 m antes das medidas (regra 5.3)' }
    if ($semCal.Count -gt 0) { throw "volta por $($semCal -join '/') sem calibracao OK: rode CALIBRAR com essa VOLTA marcada (regra 5.3)" }
    $ok = $true
} catch {
    $resultados.Add("FALHA $($_.Exception.Message)")
} finally {
    foreach ($linha in $resultados) { Write-Host $linha }
}

if ($ok) { Write-Host 'RESULTADO: OK'; exit 0 }
Write-Host 'RESULTADO: FALHOU'
exit 1
