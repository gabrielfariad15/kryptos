# Modo Direto: mensagem sem internet, de celular para celular

Decidido em 08/11/2026. Exigência de origem: `DECISOES-FASE-0.md` (seção 8) e `transportes.md`.

## Decisões de 08/11/2026

1. **Meio inicial: Bluetooth de baixo consumo (BLE)**, pelo modo padrão (GATT), com os dois celulares nos dois papéis: anunciam e procuram, aceitam conexão e conectam. O Wi-Fi Direct fica para depois, como via rápida para mensagem grande.
2. **Implementação: Kotlin próprio no app**, por um canal entre Flutter e Android como o `kryptos/tela`. Sem pacote Flutter de BLE (regra 8.6).
3. **Só com o app aberto** na primeira versão. É app de segurança e privacidade máxima, e o plantão já queima tudo quando o app sai da frente. Segundo plano, com serviço e notificação fixa, seria decisão separada.
4. **Chave avulsa pelo próprio BLE, no encontro.** Quando os dois celulares se conectam, cada um entrega chaves avulsas ao outro, no formato de 96 bytes da retirada no relay (`ChaveAvulsa::da_retirada`). O QR do contato fica como está.

## Decisões de 08/11/2026, segunda parte

5. **Abaixo do Android 12, o modo Direto fica desligado.** No Android 11 e anteriores, procurar Bluetooth exige permissão de localização. O Kryptos não pede localização em nenhuma versão: o app tem de ser intrastreável. Nem as permissões antigas (`BLUETOOTH`, `BLUETOOTH_ADMIN`) entram. O `minSdk` continua 28 para o resto do app.
6. **A divisão em pedaços definitiva vai para o núcleo em Rust**, no passo 2. Pensando adiante: o mesmo código serve ao Android, ao iPhone, aos computadores e ao rádio LoRa (`transportes.md`), e passa pela suíte. Em Kotlin, ficaria só no Android e teria de ser reescrita em Swift. Na sonda, a divisão é mínima e em Kotlin (`QuadroSonda.kt`), porque a sonda não usa o protocolo.
7. **Distância.** O próprio Bluetooth tem um modo de longo alcance: o PHY Coded (Bluetooth 5, "LE Long Range"), que troca velocidade por alcance. Entra na sonda como uma das variáveis, ao lado de 1M e 2M. A sonda também faz o inventário dos rádios de cada celular (Coded, 2M, anúncio estendido, Wi-Fi Aware, Wi-Fi Direct, NFC, UWB), para decidir os próximos meios com o que os aparelhos têm de fato.
   - **Eletrostática, recusada:** celular não tem como emitir nem ler campo elétrico para comunicação. A comunicação pelo corpo (acoplamento capacitivo) existe em pesquisa, mas exige um circuito próprio dos dois lados. O mais próximo que o celular tem é o NFC, que é indução magnética a poucos centímetros e não cobre distância.
   - **Rádio fora do Bluetooth e do Wi-Fi:** só com hardware extra, como o LoRa já previsto (`DECISOES-FASE-0.md`, seção 8).
8. **Medidas fora do cabo:** a sonda grava as linhas MEDIDA num arquivo da pasta privada do app, e o script lê no fim pelo `run-as`, que só funciona em app de debug. A depuração sem fio ficou de fora, porque exige o Wi-Fi ligado, e o Wi-Fi divide os 2,4 GHz com o Bluetooth.

## Decisões de 08/11/2026, terceira parte

9. **A sonda mede três caminhos para a volta.** A volta lenta (seção "Resultado de 08/11/2026, noite") vem de um custo fixo por notificação, não do rádio. Antes de escolher o caminho do passo 2, a sonda mede as alternativas lado a lado, com a variável VOLTA:
   - `NOTIFICACAO`: o servidor notifica (o de sempre);
   - `ESCRITA`: o servidor vira cliente GATT do cliente, pelo mesmo enlace, e escreve sem resposta, com o mesmo controle de fluxo da ida. Os papéis GATT (quem serve e quem pede) não dependem de quem conectou (Nordic, NXP);
   - `L2CAP`: canal L2CAP orientado a conexão (CoC, API 29+), sem pareamento, num PSM dinâmico que o servidor entrega pelo GATT.
   O caminho que ganhar entra no passo 2. Pode ser um por direção, ou um por par de aparelhos.
10. **Cubo: escolha automática do meio** (etapa própria, depois do passo 2). O Cubo deixa de estimar distância e passa a medir a **qualidade do enlace**:
    - Entradas: RSSI, perda de pedaço, tempo medido de cada transferência e, quando houver LoRa, a relação sinal-ruído (SNR).
    - O que escolhe: o meio (BLE 2M, BLE Coded, LoRa ou internet), quem conecta e qual caminho de volta, o ajuste do LoRa (alcance contra velocidade) e, numa malha, o repetidor.
    - Medida errada: filtro de Kalman para suavizar e RANSAC para descartar o ponto fora da curva, como na biblioteca Python do Cubo.
    - **Em Rust, no núcleo** (decisão 6): o mesmo código no Android, no iPhone, nos computadores e no rádio, e passa pela suíte. A biblioteca Python fica como referência; Python não vai para o celular.
    - **Não estima posição nem distância.** O Kryptos não deriva localização do Bluetooth, do Wi-Fi nem do som. É o que mantém verdadeiro o `neverForLocation` do `BLUETOOTH_SCAN`.
11. **Modo Local, opcional e desligado por padrão.** A mensagem só abre num raio definido (`DECISOES-FASE-0.md`, seção 8), se a pessoa ligar:
    - a permissão de localização é pedida em tempo de execução só quando a pessoa liga o Local, nunca na instalação nem no primeiro uso;
    - a distância vem só do GPS do próprio aparelho, nunca do Bluetooth, do Wi-Fi ou do som (decisão 10);
    - a coordenada não sai do aparelho;
    - nunca em segundo plano: `ACCESS_BACKGROUND_LOCATION` continua proibida.
    Hoje o `instalar-no-celular.ps1` recusa o app que tenha qualquer permissão de localização. Quando o Local for construído, essa conferência muda para: localização declarada só com o Local, não concedida na instalação, e a de segundo plano continua recusada.

## Decisões de 09/11/2026

12. **Desempate da colisão no AMBOS: pelo OLA, com um nonce por conexão.** O primeiro quadro do cliente, depois de ligar a notificação, é um OLA com um número aleatório de 64 bits, novo a cada conexão, que não identifica ninguém.
    - O servidor não escolhe mais o cliente na conexão: escolhe no OLA. Se este aparelho também tem cliente ativo, compara o nonce do outro com o do próprio cliente (`QuadroSonda.decidirOla`, comparação sem sinal).
    - A mesma regra nos dois lados faz um aceitar e parar o próprio cliente, e o outro recusar (`OLA_RECUSADO_COLISAO`). Empate (1 em 2^64): os dois recusam e a rodada recomeça.
    - Quem ainda não mandou o OLA desiste.
    - **Por que não pelo endereço:** os celulares trocam o endereço Bluetooth por privacidade. Na colisão de 09/11, a conexão que chegou do outro veio com um endereço diferente do da conexão que este abriu. A regra antiga ("um cliente por vez, recusa o outro") derrubou o cliente dos dois.
    - O bitchat resolve conexão dupla pela identidade fixa do aparelho. O Kryptos não pode, porque o anúncio não diz quem é. O nonce é a versão sem identidade, e o passo 3 ("quem é quem") vai substituir ou absorver isso.
13. **Coleta completa em todo teste da sonda pelo script:**
    - as medidas;
    - o logcat do app e do serviço de Bluetooth, com endereços trocados por apelidos (`bt-1`...) e nomes por `<oculto>`;
    - a análise;
    - a transcrição da janela, sem o cabeçalho do PowerShell;
    - bateria e temperatura no começo e no fim;
    - tudo num zip. As medidas vêm mesmo quando o roteiro falha.

## O que a pesquisa mostrou (08/11/2026)

- **bitchat** (malha BLE, Android e iPhone): cada aparelho age nos dois papéis, e a mensagem grande vai em pedaços. A Trail of Bits considerou legítimas as falhas apontadas logo no lançamento (julho de 2025), e uma auditoria posterior achou injeção de mensagem não autenticada na malha (corrigida em 28/01/2026). Lição para o Kryptos: o transporte não autentica nada; quem decide se a carta vale é o núcleo, pelo Olm.
- **Briar:** Bluetooth clássico e Wi-Fi, sem servidor, só Android.
- **Wi-Fi Direct no Android:** a conexão nova pede confirmação numa janela. No Android 10 ou mais novo dá para contornar com o hotspot de um lado e nome e senha passados ao outro (pesquisa pública do Briar). Não existe no iPhone.
- **Permissões (Android 12 ou mais novo):** `BLUETOOTH_SCAN`, `BLUETOOTH_ADVERTISE` e `BLUETOOTH_CONNECT`, pedidas em tempo de execução num aviso só ("dispositivos por perto"). Com `neverForLocation` no `BLUETOOTH_SCAN`, sem pedir localização.
- **Tamanho do pedaço:** a partir do Android 14, o primeiro pedido de MTU vira 517 bytes e os seguintes são ignorados. Os dois celulares de teste estão no Android 15. A maior carta (66.594 bytes) vai em uns 130 pedaços (estimo).
- **GATT ou canal L2CAP:** diferença de velocidade pequena segundo a Nordic; GATT é o mais bem suportado. Começa pelo GATT.
- **Segundo plano no Android 14:** serviço com notificação fixa do tipo `connectedDevice`. Fora da primeira versão (decisão 3).
- **iPhone:** com o app em segundo plano, não anuncia de forma que o Android enxergue. Entre Android e iPhone, os dois apps abertos (como o `DECISOES-FASE-0.md` previa).
- **Pacotes Flutter de BLE com os dois papéis:** novos ou de publicador não verificado. Motivo da decisão 2.

Achados do passo 1, antes do código (08/11/2026):

- **O pedaço é de 512 bytes, não de 514.** Com MTU 517, caberiam 514, mas desde o Android 13 o `writeCharacteristic` recusa com exceção valor maior que o máximo de um atributo (`GATT_MAX_ATTR_LEN`, 512). Com 3 bytes de cabeçalho, sobram 509: a carta de 66.594 bytes vai em **131 pedaços** (conta conferida no `QuadroSonda`).
- **Uma operação GATT por vez por conexão.** O Android não enfileira; a sonda espera o retorno de cada uma.
- **No servidor:** só anunciar depois do aviso de serviço adicionado (`onServiceAdded`); só notificar de novo depois do aviso de envio (`onNotificationSent`).
- **Escrita sem resposta tem controle de fluxo:** o Android só dá o retorno da escrita quando a fila interna tem espaço. O próximo pedaço sai só depois do retorno. A escrita com resposta custaria dois intervalos de conexão por pedaço.
- **Limite de procuras:** 5 `startScan` em 30 s por app; a sexta não dá erro, só não acha nada. A sonda conta e recusa antes (`PROCURA_DEMAIS`).
- **Procura com filtro pelo UUID:** a procura sem filtro para quando a tela apaga. Com `neverForLocation`, o Android pode esconder alguns beacons; o anúncio do Kryptos é um serviço próprio, e a sonda mostra se isso afeta.
- **Velocidade:** depende do intervalo de conexão (prioridade alta: 11,25 a 15 ms) e do PHY. Referência da Silicon Labs, Android para periférico, intervalo de 15 ms: 610 kbit/s em 2M e 215 kbit/s em 1M. Entre dois celulares, estimo de 1 a 3 s para 66.594 bytes; a sonda mede. O Android não mostra o intervalo de conexão na API pública.
- **O Android avisa o servidor GATT também da conexão que o próprio aparelho abriu como cliente.** No modo AMBOS, a sonda compara com o aparelho que procurou (sem gravar o endereço) para não confundir com "o outro conectou primeiro".

Achados para os caminhos de volta (08/11/2026, decisão 9):

- **L2CAP CoC no Android:** o servidor chama `listenUsingInsecureL2capChannel()`, que dá um PSM dinâmico (`getPsm()`); o cliente chama `createInsecureL2capChannel(psm)` e `connect()`. API 29 ou mais nova. O `connect()` e o `accept()` bloqueiam, então rodam em thread própria. "Insecure" é sem pareamento, como o GATT da sonda; o conteúdo do Kryptos já vai cifrado pelo Olm.
- **O canal L2CAP é um fluxo de bytes** para quem lê: não preserva a fronteira de cada escrita. A sonda põe cada quadro numa moldura com o tamanho na frente (2 bytes), e a leitura recusa moldura vazia ou maior que 512 bytes.
- **GATT nos dois sentidos:** um aparelho pode ser servidor e cliente GATT do outro ao mesmo tempo, no mesmo enlace. O MTU é do enlace; o pedido de MTU do segundo cliente pode não ter resposta, e aí a sonda usa o MTU que o servidor já conhece (`mtu_origem=herdado`). Que o Android aceite abrir esse segundo cliente no mesmo enlace é o que a sonda vai mostrar: não achei medida publicada entre dois celulares.

Fontes: developer.android.com (permissões do Bluetooth, tipos de serviço em primeiro plano, Wi-Fi Direct, mudanças do Android 14); developer.apple.com (`CBPeripheralManager.startAdvertising`); blog.trailofbits.com (18/07/2025); code.briarproject.org (public-mesh-research, issue 5); devzone.nordicsemi.com (L2CAP e GATT; MTU 517 no Android 14).

## Regras do transporte BLE

- **Entrega a carta inteira ou nada** (`transportes.md`). Divide em pedaços e remonta; quem confere se a carta vale é o núcleo, pelo `receber_carta`.
- **O anúncio não diz quem é.** Anuncia só que é um Kryptos (um identificador de serviço fixo). Quem é quem só se revela dentro da conexão, com desenho próprio antes do código (passo 3).
- **Sem localização.** `neverForLocation`; o Kryptos não deriva posição nem distância do Bluetooth (decisões 10 e 11). O Cubo usa o sinal só para escolher o meio.
- **Nada de conteúdo em log** (regra 8.2): só códigos e medidas.

## Sugestão de 08/11/2026: código Morse ou linguagem nova de criptografia

Registrada e recusada como criptografia:

- **Morse não esconde nada.** É uma tabela pública: quem tem a tabela lê. Serve para transmitir, não para proteger.
- **Cifra inventada é o erro clássico.** A segurança tem de estar só na chave, nunca no segredo do método (princípio de Kerckhoffs). Método caseiro, mesmo misturando vários códigos, não tem a análise de anos que o Olm (vodozemac, auditado) tem, e cada camada a mais é código a mais para errar.

Onde o Morse pode entrar: como **meio**, levando a carta já cifrada pelo Olm. Por exemplo, som ou luz entre dois celulares sem Bluetooth. É muito lento: estimo poucos bytes por segundo, então minutos para a menor carta (97 bytes). Fica para a etapa do som, se fizer sentido lá.

## Passos

1. **Sonda medida (código pronto em 08/11/2026; falta o teste nos celulares).** Um app de teste, montado só com `--dart-define=KRYPTOS_SONDA_BLE=true`, sem protocolo do Kryptos ainda:
   - um celular anuncia o serviço; o outro procura, conecta, pede o MTU e manda 66.594 bytes em pedaços;
   - quem recebe confere o SHA-256 dos bytes e responde;
   - mede, em linhas `MEDIDA`: tempo até achar, MTU aceito, tempo da transferência, se o SHA-256 conferiu;
   - repetir a 1, 5 e 10 metros (estimo o alcance; é o que a medida vai dizer).
   - Como ficou: seção "A sonda" abaixo.
2. **Carta de verdade:** a carta Olm passa pelo BLE e entra no `receber_carta`. Chave avulsa entregue no encontro (decisão 4).
3. **Quem é quem dentro da conexão:** como dois contatos se reconhecem sem anunciar identidade a quem está em volta. Desenho antes do código.
4. **Tela:** o modo Direto na Central, ao lado do relay.

## A sonda (passo 1)

Só existe no app de debug, montado com `--dart-define=KRYPTOS_SONDA_BLE=true`. O app de distribuição não leva o código nem as permissões de Bluetooth (conferido no manifesto final do release e do profile em 08/11/2026).

| Arquivo | O que faz |
|---|---|
| `frontend/app/android/app/src/debug/kotlin/.../SondaBle.kt` | Servidor e anúncio, procura e cliente, as medidas |
| `frontend/app/android/app/src/debug/kotlin/.../QuadroSonda.kt` | Quadros da sonda e remontagem, em Kotlin puro (sem Android) |
| `frontend/app/android/app/src/{debug,profile,release}/kotlin/.../CanaisDeTeste.kt` | Liga o canal `kryptos/sonda-ble` no debug; vazio no profile e no release |
| `frontend/app/android/app/src/debug/AndroidManifest.xml` | `BLUETOOTH_SCAN` com `neverForLocation`, `BLUETOOTH_ADVERTISE`, `BLUETOOTH_CONNECT`; nada de localização |
| `frontend/app/lib/sonda_ble.dart` | Tela: papel, variáveis, linhas |
| `ferramentas/instalar-no-celular.ps1 -SondaBle` / `-LerSonda` | Monta, confere, instala, zera, espera FIM, traz as medidas e analisa |
| `ferramentas/instalar-no-celular.ps1 -SondaBle -Conferir` / `-Autonomo` / `-Alcance` | O mesmo sem tocar na tela: permissão pelo `pm grant` e a sonda conduzida pelo `sonda-roteiro.ps1` |
| `ferramentas/sonda-roteiro.ps1` | Manda os comandos à sonda pelo cabo e espera cada fim (chamado pelo de cima) |
| `ferramentas/analisar-sonda-ble.ps1` | Resumo das medidas, por distância, PHY, prioridade e caminho de volta |

Nenhuma dependência nova: só a API do Android (platform 36). A sonda exige Android 13 ou mais novo (as chamadas de escrita e notificação que devolvem código); os dois celulares de teste são Android 15.

**Papéis:**

- **ANUNCIAR (servidor):** abre o serviço e anuncia em dois conjuntos, o legado (1M, o que qualquer aparelho vê) e o de longo alcance (Coded), se o aparelho tiver. O anúncio leva só o UUID do serviço, com potência alta.
- **PROCURAR (cliente):** procura, conecta, pede MTU 517 uma vez, ajusta PHY e prioridade, liga a notificação e faz 5 idas e 5 voltas da carta de 66.594 bytes aleatórios.
  - Ida: o cliente escreve sem resposta; o servidor confere o SHA-256 e responde.
  - Volta: o servidor manda pelo caminho escolhido em VOLTA (decisão 9); o cliente confere. Com ESCRITA ou L2CAP, antes da primeira ida o cliente pede `PREPARAR` e espera o `PRONTO`:
    - ESCRITA: o cliente abre um serviço GATT próprio (não anunciado), e o servidor conecta nele pelo mesmo enlace;
    - L2CAP: o servidor abre a escuta e manda o PSM no `PRONTO`; o cliente conecta o canal.
  - Os quadros de controle (resultado da ida, `PREPARAR`, `PRONTO`, pedido da volta) vão sempre pelo GATT de sempre. Só a carta da volta muda de caminho, com o mesmo pedaço de 509 bytes nos três: o que vai no ar são pacotes de até 251 bytes em qualquer caso.
- **CALIBRAR:** como o PROCURAR, com 3 idas e voltas: na 1ª um byte trocado, na 2ª um pedaço a menos, na 3ª nada. Esperado: `SHA_NAO_CONFERE`, `PEDACO_FALTANDO` e `CONFERIU`, nos dois sentidos. Sem calibração OK, a análise dá FALHA (regra 5.3).
- **AMBOS (segunda rodada):** os dois papéis ao mesmo tempo; o desempate é uma espera aleatória de até 1,5 s antes de conectar, sem nenhum dado a mais no anúncio.
- **Ciclo de vida:** a tela fica acesa enquanto a sonda está aberta. Se o app sai da frente, a sonda para tudo (`APP_SAIU_DA_FRENTE`).
- **ENVIAR MEDIDAS:** abre o "Compartilhar" do Android com o arquivo de medidas inteiro, em texto (WhatsApp, e-mail, copiar). Só com a sonda parada, porque abrir a escolha tira o app da frente. Até 400 KB; acima disso, use o `-LerSonda`. Sem medidas, a tela mostra `ERRO SEM_MEDIDAS`.

**Sem tocar na tela (08/11/2026).** O script grava uma linha `id=<n> acao=<acao> chave=valor ...` em `files/sonda-comando.txt`, na pasta privada do app, pelo `run-as`: só pelo cabo e só no app de debug, então nenhum outro app do celular consegue mandar a sonda ligar o Bluetooth. A sonda lê a cada 250 ms, marca `comando` ao receber e `comando-fim` (com o desfecho) ao terminar. Com o app fora da frente, recusa (`COMANDO_FORA_DA_FRENTE`, decisão 3). O arquivo chega por `adb exec-in run-as ... tee` ou, se não der, por `adb push` e `run-as cp`; o script testa nessa ordem e diz qual valeu. A permissão "dispositivos por perto" vai pelo `pm grant`, conferida no `dumpsys`.

- **`-Conferir`** (um celular basta): o comando chega; o L2CAP abre escuta; anuncia; recusa comando errado; para; carrega e cancela o roteiro; procura sozinho e dá `NAO_ACHOU_30S`; recusa com o app fora da frente e aceita de volta.
- **`-Autonomo`** (os dois no cabo, a 1 m): calibra os três caminhos de volta e mede 1M PADRAO, 2M ALTA nos três caminhos e Coded ALTA, nos dois sentidos; AMBOS 3 vezes, os dois soltos juntos; ciclo de vida com o botão de início no meio das idas. Estimo de 15 a 20 min.
- **`-Ambos`** (os dois no cabo, a 1 m): uma calibração e 10 rodadas de AMBOS de 2 idas e voltas: 3 com a espera aleatória, 3 com espera 0 (os dois conectam juntos) e 4 com espera 0 e o OLA segurado 2 s (os dois clientes se apresentam, e o desempate é pela comparação do nonce) (decisão 12). Esperado em todas: um `ok` e um `servidor`. Dois `ok` saem como `DOIS CLIENTES`.
- **`-Alcance`**: o primeiro celular anuncia; o segundo calibra a 1 m (sem `-Autonomo`) e recebe o roteiro (5, 10, 15, 20 e 30 m; 1M PADRAO e Coded ALTA; 3 repetições). Sem cabo, em cada ponto a pessoa toca **PRÓXIMO PONTO** no celular que procura. O roteiro para sozinho no ponto em que nenhuma combinação achou o outro (`roteiro-fim motivo=SEM_ALCANCE_A_<d>M`).

**Variáveis**, escolhidas na tela, uma por rodada: PHY (1M, 2M, CODED), prioridade (PADRAO, ALTA), caminho de volta (NOTIFICACAO, ESCRITA, L2CAP) e a distância declarada (1, 5, 10, 15, 20, 30 m). Quem escolhe é o celular que procura.

**Linhas.** O formato é `<hora> MEDIDA sonda-ble <evento> chave=valor ...` e `<hora> FALHA sonda-ble <CODIGO> ...`. Elas vão para o logcat (tag `KryptosSonda`), para a tela e para `files/sonda-ble.txt`, na pasta privada do app (no máximo 2 MiB). Só códigos e números: nem o endereço Bluetooth do outro aparelho.

| Evento | Campos principais |
|---|---|
| `aparelho` | modelo, android, api, le2m, coded, anuncio_estendido, anuncio_max_bytes, anuncios_multiplos, wifi_aware, wifi_direct, nfc, uwb |
| `anuncio` | tipo (legado, coded), resultado, tx_dbm |
| `achou` | rodada, distancia, phy, prioridade, volta, calibrar, phy_anuncio, rssi, ms |
| `conexao` | ms, mtu, pedaco, pedacos, phy_tx, phy_rx |
| `volta-pronta` | (cliente) ms para abrir o caminho ESCRITA ou L2CAP; tx_max e rx_max no L2CAP |
| `servidor-volta` | (servidor) caminho, ms; mtu e mtu_origem na ESCRITA; tx_max e rx_max no L2CAP |
| `ida`, `volta` | n, bytes, pedacos, ms, kbps, resultado, recebidos, ms_receptor, ocupado, rssi (só na ida) |
| `servidor-recebeu`, `servidor-enviou` | o mesmo, do lado do servidor; `servidor-enviou` leva o caminho (no L2CAP, ocupado e mtu saem `-`) |
| `rodada-fim`, `calibracao`, `ambos`, `parou` | o desfecho de cada rodada; `calibracao` leva volta (o caminho) e as listas ida e voltas |
| `ola` | (cliente) resultado do OLA e ms até a resposta |
| `ambos` | desfecho (`cliente`, `servidor`, `colisao`) e, na colisão, `este=` o papel que ficou |
| `comando`, `comando-fim` | id e acao; desfecho (`ok`, `calibracao_OK`, `servidor`, `falha`, `nao_comecou`, `recusado`) e o código |
| `l2cap-teste` | resultado, psm (um aparelho só, `-Conferir`) |
| `roteiro`, `roteiro-ponto`, `roteiro-fim` | distâncias e combinações carregadas; começo e fim de cada ponto; motivo do fim |

Códigos de FALHA: `ANDROID_ANTIGO`, `SEM_PERMISSAO`, `SEM_BLUETOOTH`, `BLUETOOTH_DESLIGADO`, `PROCURA_DEMAIS`, `NAO_ACHOU_30S`, `CONEXAO_CAIU` (com o status), `TEMPO_ESGOTADO` (com a etapa), `MTU`, `ESCRITA*`, `NOTIFICACAO*`, `ANUNCIO_LEGADO`, `ANUNCIO_CODED`, `QUADRO_INVALIDO`, `APP_SAIU_DA_FRENTE`, `PARADA_PEDIDA`.

Do OLA: `OLA_RECUSADO` (servidor já ocupado), `QUADRO_SEM_OLA` (quadro de quem não se apresentou), `SEM_CLIENTE`.

Dos comandos: `COMANDO_INVALIDO` (com o motivo), `COMANDO_FORA_DA_FRENTE`, `CLIENTE_JA_ATIVO`, `L2CAP_ESCUTA`.

Dos caminhos de volta: `VOLTA_PREPARAR` (com o motivo, do lado do servidor), `VOLTA_NAO_PREPARADA`, `VOLTA_CAIU`, `SERVIDOR_VOLTA_NAO_ABRIU`, `SERVICO_VOLTA_RECUSADO`, `L2CAP_CONEXAO`, `L2CAP_CAIU`, `L2CAP_ESCRITA`.

**Como foi provado antes do celular (08/11/2026):**

- `QuadroSonda.kt`: 33 casos fora do Android, com MTU 23 a 517, tamanhos de borda e quadros fora do formato. Quatro sabotagens, uma por vez, cada uma derrubou pelo menos um caso: sem conferir o SHA, sem o teto de 512, sem conferir falta de pedaço e sem conferir o tamanho do pedaço.
- Tela (`test/sonda_ble_test.dart`): 7 testes; três sabotagens, uma por vez, cada uma pega.
- `analisar-sonda-ble.ps1`: os números de um caso fabricado conferem com a conta à mão. Pegou: calibração que falhou, sem calibração, arquivo vazio, arquivo só com lixo, arquivo que não existe.
- `instalar-no-celular.ps1 -SondaBle` e `-LerSonda`, numa cópia isolada com `adb` e `flutter` falsos, o APK de verdade e o `aapt2` de verdade. Caso certo de ponta a ponta. Pegou:
  - um celular só;
  - combinado com `-Relay`, e `-SondaBle` com `-LerSonda`;
  - Bluetooth desligado;
  - APK com localização, sem `neverForLocation` ou sem `BLUETOOTH_CONNECT`;
  - app instalado com localização;
  - arquivo de medidas que não apagou, vazio ou ausente;
  - celular que não voltou ao cabo em 180 s;
  - análise sem calibração.

  O `-LerSonda` não instala, não zera e não compila. A captura de tela não roda no Linux (System.Drawing só existe no Windows) e ficou fora dessa calibração.
- Kotlin das três variantes compilado, e a suíte agora compila sempre (passo "kotlin do Android").
- **Caminhos de volta (08/11/2026, decisão 9):**
  - `QuadroSonda.kt`: 53 casos fora do Android (eram 33). Os novos cobrem `PREPARAR`, `PRONTO` com PSM até 65.535, o pedido da volta com o caminho, e a carta inteira passando em molduras por um fluxo que entrega no máximo 7 bytes por leitura, como um canal picotado. Duas sabotagens, cada uma pegou: sem recusar caminho desconhecido e sem recusar moldura vazia ou maior que 512 bytes.
  - Tela: os testes conferem que a VOLTA escolhida vai para o Android; sem o campo, dois falham.
  - `analisar-sonda-ble.ps1`: separa ida e volta pelo caminho e pelo celular que procura, e exige uma calibração OK para cada caminho medido. Pegou: L2CAP e ESCRITA medidos só com calibração de notificação, e calibração da ESCRITA que falhou. Os arquivos de antes (sem o campo) continuam dando o mesmo resultado, como NOTIFICACAO.

- **Defeito que o celular achou e a calibração deixou passar (08/11/2026):** a conferência do arquivo zerado usava `run-as ... sh -c "if ...; then ...; fi"`. O `adb shell` junta os argumentos numa linha só, e o shell do celular lê essa linha de novo: o `;` quebrou o comando (`syntax error: unexpected 'then'`). O `adb` falso da calibração executava os argumentos direto, sem essa segunda leitura. Agora a conferência é um `ls` de um arquivo só, sem nada para o shell interpretar, e o `adb` falso imita a segunda leitura: com ele, o script anterior falha do mesmo jeito que no celular, e o corrigido passa. Casos errados: arquivo que não apagou, `run-as` que falha e resposta inesperada do `ls`. Nenhum outro `adb shell` do script tem caractere que o shell reinterprete.

- **Sem tocar na tela (08/11/2026):** `-Conferir`, `-Autonomo` e `-Alcance` com `adb` falso e uma sonda falsa que responde como o `SondaBle.kt`, 22 casos. Certos: um celular, dois celulares, 1 m, alcance, os dois juntos, e o `exec-in` mudo ou quebrado caindo no `push`. Errados, cada um com o motivo certo na saída: nenhuma forma de levar o comando; `pm grant` que não pegou ou foi recusado; tela que não abre; sonda que não responde; L2CAP sem escuta; sonda que aceita comando fora da frente; procura sozinha que não dá `NAO_ACHOU_30S`; comando sem fim; calibração de um caminho que falhou; um celular só no `-Autonomo`; opções que não combinam. O `-SondaBle` à mão de antes continua igual. Tela: o PRÓXIMO PONTO só aparece quando a sonda espera a pessoa; sem essa condição, o teste falha.
- **Desempate e coleta (09/11/2026):**
  - `decidirOla`: 100.000 pares de nonces sorteados, sempre um cliente só. Duas sabotagens pegas: comparação com sinal, e "quem ainda não mandou OLA" sem desistir.
  - Script, com `adb` falso que imita o logcat (endereços, um nome de fone, linhas de outro app, um trecho só no buffer): 31 casos.
    - Certos: `-Ambos` dá 10 de 10 com 5 colisões resolvidas; o zip tem medidas, logcat, análise e transcrição, mesmo quando o roteiro falha.
    - Errados: dois clientes depois da colisão saem como `DOIS CLIENTES`; sem calibração, a análise recusa (o `-Ambos` calibra antes por isso).
    - Quatro sabotagens da coleta, cada uma pega: sem máscara, sem o filtro por processo, transcrição com cabeçalho, sem o buffer.
    - Defeito achado nessa calibração e corrigido: com a máscara falhando, o arquivo sem máscara ficava em Downloads. Agora é apagado.
- **A captura de tela** não é conferida fora do Windows (System.Drawing): o passo diz `SEM CONFERENCIA` em vez de quebrar.

O que não dá para provar aqui: o Bluetooth de verdade, e se o `exec-in` ou o `run-as cp` passam no SELinux de cada aparelho. Isso é o `-Conferir` e o teste abaixo.

## Teste da sonda

Sem tocar na tela (recomendado):

1. Com um celular só no cabo, primeiro: `powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -SondaBle -Conferir`.
   - Desempate da colisão (decisão 12), os dois no cabo: `... -Todos -SondaBle -Ambos`.
2. Com os dois no cabo, a 1 m um do outro: `... -Todos -SondaBle -Autonomo`.
3. Alcance, com a trena: `... -Todos -SondaBle -Alcance` (ou `-Autonomo -Alcance` junto). O primeiro celular da lista do adb anuncia e o segundo procura: o script mostra qual é qual.

O roteiro à mão abaixo continua valendo.

Antes:

1. Nos dois celulares: Não perturbe ligado e Bluetooth ligado. Deixe o Wi-Fi como está e não mexa nele durante o teste: ele divide os 2,4 GHz com o Bluetooth.
2. Os dois no cabo do PC. Uma linha:

   ```
   powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -SondaBle
   ```

   Rode até aparecer `Digite FIM para encerrar`, e não digite FIM ainda.

Roteiro (Edge = motorola edge 40 neo, g15 = moto g15):

1. Nos dois: no aviso "Dispositivos por perto", toque em Permitir. A primeira linha da tela é o inventário do aparelho.
2. Edge: ANUNCIAR. Na tela, `anuncio tipo=legado resultado=ok` e, se ele tiver, `tipo=coded`.
3. g15, a 1 m: PHY 1M, PRIORIDADE PADRAO, DISTÂNCIA 1, depois CALIBRAR. Espere `calibracao ... resultado=OK`. Se vier FALHOU, pare e me mande.
4. g15, ainda a 1 m: uma rodada de PROCURAR para cada combinação. Espere `rodada-fim` e mais 6 s antes da próxima (limite de 5 procuras em 30 s):
   - 1M PADRAO;
   - 1M ALTA;
   - 2M ALTA;
   - CODED ALTA (se algum dos dois não tiver Coded, a linha diz; pule).
5. Tire os cabos. Repita o passo 4 a 5 m e a 10 m: trena, linha reta, sem parede, e marque a DISTÂNCIA na tela. **Os dois celulares têm de estar de fato separados por essa distância, cada um num ponto.** Trocar só a etiqueta na tela não mede nada (08/11/2026), e o analisador avisa quando o sinal não caiu.
6. Alcance: com 1M PADRAO e com CODED ALTA, a 15, 20 e 30 m, até aparecer `NAO_ACHOU_30S` ou `CONEXAO_CAIU`.
7. Papéis trocados, a 1 m: PARAR nos dois; g15 ANUNCIAR; Edge PROCURAR com 2M ALTA.
8. Os dois papéis (segunda rodada), a 1 m: PARAR nos dois; nos dois, PHY 1M e AMBOS quase ao mesmo tempo. Espere `rodada-fim` ou uma FALHA e toque PARAR nos dois. Faça 3 vezes.
9. Ciclo de vida: Edge ANUNCIAR; g15 PROCURAR com 1M PADRAO. Durante as idas, aperte o botão de início no g15. Esperado: `FALHA ... APP_SAIU_DA_FRENTE` no g15 e `servidor evento=desconectou` no Edge. Volte ao app.
10. Caminhos de volta, a 1 m (decisão 9). Edge ANUNCIAR; no g15, 2M ALTA:
    - VOLTA ESCRITA, CALIBRAR; depois PROCURAR;
    - VOLTA L2CAP, CALIBRAR; depois PROCURAR;
    - VOLTA NOTIFICACAO, PROCURAR (para comparar no mesmo momento).
    Depois, PARAR nos dois, g15 ANUNCIAR e repetir os três PROCURAR no Edge. Sem calibração do caminho, a análise dá FALHA.
11. Ligue os dois de volta no cabo e só então digite FIM.

O script traz os dois arquivos de medidas para Downloads, num zip `kryptos-sonda-<data>.zip`, e roda a análise. Cole as linhas de saída e anexe o zip.

Se o script parar antes de trazer as medidas: com os dois no cabo, rode a mesma linha trocando `-SondaBle` por `-LerSonda`. As medidas só são apagadas no próximo `-SondaBle`.

Duração: estimo de 25 a 40 min.

## Resultado de 08/11/2026, a 1 m

Edge anunciando, g15 procurando, os dois no Wi-Fi de casa, a 1 m. Arquivos `kryptos-sonda-*-20261108-142106.txt`.

- **Inventário:** os dois têm 2M, Coded e anúncio estendido; nenhum tem Wi-Fi Aware nem UWB. O anúncio cabe em até 304 bytes no Edge e 192 no g15. O Edge informou potência de −3 dBm com "potência alta".
- **Calibração:** OK na rodada 3: byte trocado, pedaço a menos e normal pegos nos dois sentidos.
- **Conexão:** MTU 517 aceito, pedaço de 509 bytes, 131 pedaços por carta. A fila de escrita nunca ficou ocupada (`ocupado=0` em todas).
- **Integridade:** fora da calibração, 16 idas e 15 voltas, todas conferidas. Nenhum pedaço perdido.
- **Achar:** de 107 a 393 ms. **Conectar:** de 221 ms (prioridade alta) a 1.073 ms (padrão).
- **RSSI a 1 m:** de −40 a −56 dBm.

Carta de 66.594 bytes, mediana de 5 (ms e kbit/s):

| Combinação | Ida (cliente escreve sem resposta) | Volta (servidor notifica) |
|---|---|---|
| 1M, prioridade padrão | 2.427 ms, 220 kbit/s | 20.028 ms, 26,6 kbit/s |
| 1M, prioridade alta | 1.018 ms, 523 kbit/s | 6.335 ms, 84,1 kbit/s |
| 2M, prioridade alta | 583 ms, 914 kbit/s | 6.156 ms, 86,5 kbit/s |

O que isso diz:

- **A ida bate com a referência e até passa dela:** 914 kbit/s em 2M, contra 610 kbit/s da Silicon Labs. A prioridade alta mais que dobra a velocidade; o 2M quase dobra de novo.
- **A volta é de 6 a 10 vezes mais lenta, e o 2M não ajudou** (6,3 s contra 6,2 s). Então o gargalo não é o rádio: é o servidor esperar o `onNotificationSent` de cada notificação. Dá uma notificação a cada 47 ms com prioridade alta (6,2 s / 132 quadros) e a cada 152 ms com a padrão.
- **Para mensagem comum, isso quase não pesa:** a carta de texto tem de 97 a 396 bytes (`transportes.md`) e cabe num pedaço só, então estimo dezenas de ms nos dois sentidos. Pesa na carta grande (66 KB) e no que vier depois (anexo, chaves em lote).
- **Coded:** o g15 achou o anúncio de longo alcance (393 ms, −44 dBm), mas a conexão caiu em 0,5 s com status 133 (`GATT_ERROR`, genérico). Foi uma tentativa só, e não dá para concluir com isso.
- **Defeito da sonda (corrigido no mesmo dia):** na rodada 2, o PEDIR_VOLTA chegou ao servidor antes do aviso de envio do RESULTADO. O servidor recusou (`NOTIFICACAO_EM_ANDAMENTO`), e a volta esgotou o tempo. Agora as notificações entram numa fila. A corrida depende do tempo de cada celular e não se reproduz aqui; quem prova é a próxima rodada nos celulares.
- **Ciclo de vida:** o Edge saiu da frente às 14:34:49, e a sonda parou tudo, como desenhado.

Faltam: 5, 10, 15, 20 e 30 m; mais tentativas de Coded; papéis trocados; os dois papéis ao mesmo tempo.

## Resultado de 08/11/2026, noite (versão com a fila corrigida)

Arquivos `kryptos-sonda-*-20261108-202820.txt`. **Todas as rodadas foram com os celulares juntos, a ~1 m.** A DISTÂNCIA marcada na tela (5 a 30 m) era só a etiqueta, confirmado pelo Gabriel. O RSSI mostrou isso: −35 a −45 dBm em todas as "distâncias". Então não há medida de alcance ainda. A partir de agora o `analisar-sonda-ble.ps1` avisa quando o sinal numa distância maior fica a menos de 6 dB do sinal a 1 m do mesmo aparelho, e tira essa distância do alcance.

- **Fila corrigida:** nenhum `NOTIFICACAO_EM_ANDAMENTO` (eram 13 na tarde, com a versão antiga). Calibração: 7 de 7 OK, inclusive em Coded.
- **A volta lenta depende de qual celular conecta.** Carta de 66.594 bytes, mediana:

| Cliente (conecta) | Rádio, prioridade | Ida (cliente escreve) | Volta (o outro notifica) |
|---|---|---|---|
| g15 | 1M, padrão (tarde) | 2,4 s | 20,0 s |
| g15 | Coded, alta (12 envios) | 7,3 s | 6,4 s |
| Edge | 1M, padrão (5 envios) | 1,17 s | 1,14 s |
| Edge | Coded, padrão (5 envios) | 5,4 s | 4,6 s |

  - Com o Edge conectando, as duas direções andam juntas, ~460 kbit/s em 1M, prioridade padrão.
  - Com o g15 conectando, o Edge notificando custa ~6,3 s com prioridade alta, seja 1M, 2M ou Coded: um custo fixo por notificação, não o rádio.
  - Com dois celulares não dá para separar se o lento é o Edge notificar ou o g15 conectar: as duas coisas trocam juntas.
- **Coded:** conectou em 7 de 9 tentativas do g15. O status 133 é intermitente (1 vez), não constante.
- **Cada aparelho lê o RSSI num nível próprio:** na mesma situação, o Edge leu uns 12 dB abaixo do g15. Comparação de alcance só vale de um aparelho com ele mesmo.
- **Os dois papéis ao mesmo tempo (AMBOS):** ainda não testado. Nas duas rodadas, só o Edge estava em AMBOS; o g15 estava anunciando. O Edge não confundiu a própria conexão com "o outro conectou primeiro".

Faltam: alcance com os celulares separados de verdade, na trena, os dois fora do cabo; AMBOS com os dois tocando juntos; e as alternativas para a volta (escrita nos dois sentidos e canal L2CAP), aprovadas e construídas no mesmo dia (decisão 9, passo 10 do teste).

## Resultado de 09/11/2026, `-Conferir` no Edge (um celular)

Arquivo `kryptos-sonda-motorola-edge-40-neo-20261109-000333.txt`; 8 de 8 conferências OK, sem tocar na tela.

- **Comando por arquivo:** chegou pelo `exec-in` (o `push` não foi preciso). Do comando gravado à linha `comando`: o intervalo de leitura de 250 ms mais o adb; de um comando ao próximo, uns 2 s, que é a espera do próprio script entre leituras.
- **Permissão pelo `pm grant`:** as três concedidas, sem o aviso na tela. Com isso, a captura voltou a ser conferida: centro preto em 1.600 de 1.600 pontos, FLAG_SECURE valendo também na sonda.
- **L2CAP:** a escuta abriu sem pareamento, PSM 128 (0x80, o primeiro da faixa dinâmica do BLE).
- **Anúncio:** legado e Coded, os dois com −3 dBm em potência alta (o mesmo de 08/11).
- **Procura sem o outro celular:** `NAO_ACHOU_30S` em 30,07 s, sem localização, e o fim marcado.
- **Fora da frente:** com o botão de início, o comando foi recusado (`COMANDO_FORA_DA_FRENTE`); de volta à frente, aceito.

Não mede nada do enlace: os caminhos de volta, a volta lenta e o alcance continuam esperando os dois celulares (`-Todos -SondaBle -Autonomo`, depois `-Alcance`).

## Resultado de 09/11/2026, `-Autonomo` a 1 m (os três caminhos de volta)

Arquivos `kryptos-sonda-*-20261109-003458.txt`, sem tocar na tela, os dois no Wi-Fi de casa, a 1 m.

- **Calibração:** 6 de 6 OK, os três caminhos nos dois sentidos.
- **Integridade:** fora da calibração, todas as idas e voltas que terminaram conferiram. Nenhum pedaço perdido.
- **Volta, carta de 66.594 bytes, mediana de 5:**

| Quem procura (conecta) | Rádio, prioridade | NOTIFICACAO | ESCRITA | L2CAP |
|---|---|---|---|---|
| g15 | 2M, alta | 6.035 ms | 6.111 ms | 4.282 ms |
| Edge | 2M, alta | 549 ms | 557 ms | 491 ms |
| g15 | 1M, padrão | 20.417 ms | — | — |
| Edge | 1M, padrão | 1.167 ms | — | — |

  A ida (quem procura escreve) ficou entre 595 e 766 ms em 2M alta, nos dois sentidos.

**O que isso diz:**

- **O lento é um sentido do enlace, não a API.** Com o g15 conectando, o Edge manda devagar pelos três caminhos; com o Edge conectando, os três são rápidos. A escrita do servidor (ESCRITA) não ganhou nada da notificação.
- **A conta bate com um pacote do rádio por evento de conexão** (estimo, pela aritmética; o Android não mostra o intervalo):
  - Pelo GATT, cada quadro de 512 bytes vira 519 com os cabeçalhos, ou 3 pacotes de até 251 bytes: 132 quadros dão 396 pacotes. 6,0 s / 396 = 15 ms, o intervalo da prioridade alta (11,25 a 15 ms). 20,4 s / 396 = 52 ms, perto do intervalo da padrão (30 a 50 ms).
  - Pelo L2CAP, o fluxo enche os pacotes: uns 270 pacotes. 4,3 s / 270 = 16 ms, de novo um por evento. Por isso o L2CAP ganha 29% no sentido lento: só manda menos pacotes.
  - No sentido rápido, cabem vários pacotes por evento.
- **Quem limita, o Edge como periférico ou o g15 como central, não dá para separar com dois celulares.** Um terceiro aparelho separa.
- **Primeira conexão de volta pela ESCRITA, nos dois sentidos:** ida e volta ficaram de 2 a 3 vezes mais lentas, como se o intervalo voltasse ao padrão. Nas rodadas seguintes, normal. Palpite: o Android reajusta os parâmetros do enlace quando o segundo cliente GATT conecta pela primeira vez.
- **L2CAP:** abre em 51 a 81 ms do lado de quem anuncia e é o mais rápido nos dois sentidos (até 1.089 kbit/s).
- **Coded, alta:** com o Edge conectando, 5 de 5 (ida 7,4 s, volta 2,6 s; a conexão levou 5,7 s). Com o g15 conectando, a volta esgotou os 30 s.
- **AMBOS:** 2 de 3. Nas duas boas, o Edge ficou de cliente. Na terceira, os dois conectaram ao mesmo tempo (`duas_conexoes`). Cada servidor recusou a conexão do outro como "outro cliente", a notificação falhou com status 129 e as duas rodadas esgotaram o tempo. O desempate por espera aleatória não cobre a colisão.
- **Ciclo de vida:** não testado nesta rodada. A procura logo depois do AMBOS caiu com 133 em 0,4 s, e o script esperou a primeira ida em vez de ver o fim. Defeito do script, corrigido no mesmo dia: agora reconhece o fim da rodada, tenta de novo uma vez e, se repetir, dá AVISO (`ciclo de vida sem teste`). Calibrado nos dois casos, 133 uma vez e 133 sempre.

## Resultado de 09/11/2026, `-Ambos` (desempate pelo OLA)

Arquivos `kryptos-sonda-*-20261109-021400`, primeira coleta completa (medidas, logcat mascarado, análise, transcrição).

- **10 de 10 rodadas com um cliente só,** incluindo as 5 com espera 0. Antes do OLA, a colisão derrubava os dois.
- **A colisão aconteceu de verdade em 8 das 10.** O contador do script dizia "nenhuma" porque só contava `desfecho=colisao`. O logcat mostra o que houve, por exemplo na rodada 6 do Edge:
  - o Edge começou a conectar no g15 pelo endereço do anúncio (`bt-20`);
  - o g15 conectou no Edge com outro endereço (`bt-13`);
  - ficaram dois enlaces físicos ao mesmo tempo (handles 513 e 512);
  - o OLA do g15 chegou quando o cliente do Edge ainda pedia o MTU, e o Edge desistiu (`ambos desfecho=cliente` e logo `desfecho=servidor`).
  - É a confirmação, no log do Android, de que o endereço não serve para desempatar.
- **Todas as 8 se resolveram pelo ramo "quem ainda não mandou o OLA desiste".** A comparação dos nonces não chegou a rodar no celular: o g15 acha e conecta antes, e o OLA dele chega primeiro. Para testar esse ramo, o comando `ambos` ganhou `atraso_ola_ms` (o servidor segura o OLA, e os dois clientes chegam a se apresentar). O contador agora separa "antes do OLA" de "pelo nonce".
- **Quem ganha a corrida:** o g15 ficou de cliente em 9 das 10. Com ele conectando, o Edge manda no sentido lento: volta de 19,2 s em 1M padrão, contra 1,16 s com o Edge conectando.
- **Coleta:**
  - logcat com 2.627 (Edge) e 2.811 (g15) linhas do app e do serviço de Bluetooth, de 37 mil e 48 mil no total;
  - 30 e 44 endereços mascarados; nenhum endereço nem nome sobrou.
  - No nível de log padrão, o Android não registra o intervalo de conexão: a explicação "um pacote por evento" continua sendo conta, não medida. O registro HCI do Android mostraria, mas só sai pelo bugreport, que carrega dados pessoais do celular. Ficou de fora.
  - A transcrição trazia "TerminatingError" do `pidof` de um processo que não existe nesses celulares (não era erro). Corrigido, e a calibração agora confere que a transcrição sai sem isso.

## O que não resolve

- **Presença:** quem procura BLE por perto vê que há um Kryptos ali (o identificador de serviço é fixo). Não vê quem é.
- **Alcance:** ainda não medido (os celulares estavam juntos). Velocidade só a 1 m.
- **Caminhos de volta:** medidos a 1 m em 09/11/2026 (seção acima); em distância, não. O Android aceitou o segundo cliente GATT no mesmo enlace, mas a escrita não escapou do sentido lento, e o L2CAP só melhorou 29%. O sentido lento é do enlace aberto pelo g15: nenhum dos três resolve. Com dois celulares, continua sem dar para separar aparelho de papel.
- **Desempate pelo OLA:** cobre dois celulares. Com três ou mais (malha), cada par se resolve assim, mas a escolha de com quem conectar fica para o Cubo (decisão 10). O nonce não autentica nada: um aparelho mal-intencionado pode sempre mandar o maior e ficar de cliente. Na sonda isso não importa; no Kryptos, o passo 3 decide.
- **Logcat mascarado:** endereço Bluetooth sempre; nome de aparelho só quando vem como `name=`, `nome=` ou `alias=`. Nome em outro formato, no log do serviço de Bluetooth (fones, carro), passaria. As linhas são só do app e do serviço de Bluetooth.
- **L2CAP sem pareamento:** o enlace não é cifrado (como o GATT da sonda). No Kryptos isso não expõe conteúdo, que vai cifrado pelo Olm, mas quem escuta o rádio vê o tamanho e a hora de cada carta.
- **Distância declarada:** a sonda não mede distância. A trena é a referência, e o RSSI é só a leitura do sinal naquele lugar, não posição.
- **Dois modelos só:** outro chip pode se comportar diferente, principalmente com os dois papéis ao mesmo tempo e com o Coded.
- **Wi-Fi ligado ou desligado** muda o Bluetooth (2,4 GHz). O script registra o estado, mas não controla.
- **Android 12 (API 31 e 32):** o modo Direto da versão final vale do Android 12 em diante, mas a sonda exige o 13. A diferença fica para o passo 2.
- **iPhone:** fora desta etapa.
- **Segundo plano:** fora da primeira versão (decisão 3).
