# Modo Direto: mensagem sem internet, de celular para celular

Decidido em 08/11/2026. Exigência de origem: `DECISOES-FASE-0.md` (seção 8) e `transportes.md`.

## Decisões de 08/11/2026

1. **Meio inicial: Bluetooth de baixo consumo (BLE)**, pelo modo padrão (GATT), com os dois celulares nos dois papéis: anunciam e procuram, aceitam conexão e conectam. O Wi-Fi Direct fica para depois, como via rápida para mensagem grande.
2. **Implementação: Kotlin próprio no app**, por um canal entre Flutter e Android como o `kryptos/tela`. Sem pacote Flutter de BLE (regra 8.6).
3. **Só com o app aberto** na primeira versão. É app de segurança e privacidade máxima, e o plantão já queima tudo quando o app sai da frente. Segundo plano, com serviço e notificação fixa, seria decisão separada.
4. **Chave avulsa pelo próprio BLE, no encontro.** Quando os dois celulares se conectam, cada um entrega chaves avulsas ao outro, no formato de 96 bytes da retirada no relay (`ChaveAvulsa::da_retirada`). O QR do contato fica como está.

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

Fontes: developer.android.com (permissões do Bluetooth, tipos de serviço em primeiro plano, Wi-Fi Direct, mudanças do Android 14); developer.apple.com (`CBPeripheralManager.startAdvertising`); blog.trailofbits.com (18/07/2025); code.briarproject.org (public-mesh-research, issue 5); devzone.nordicsemi.com (L2CAP e GATT; MTU 517 no Android 14).

## Regras do transporte BLE

- **Entrega a carta inteira ou nada** (`transportes.md`). Divide em pedaços e remonta; quem confere se a carta vale é o núcleo, pelo `receber_carta`.
- **O anúncio não diz quem é.** Anuncia só que é um Kryptos (um identificador de serviço fixo). Quem é quem só se revela dentro da conexão, com desenho próprio antes do código (passo 3).
- **Sem localização.** `neverForLocation`; o Kryptos não deriva posição do Bluetooth. O Cubo (estimar distância) é outra etapa e terá decisão própria sobre isso.
- **Nada de conteúdo em log** (regra 8.2): só códigos e medidas.

## Sugestão de 08/11/2026: código Morse ou linguagem nova de criptografia

Registrada e recusada como criptografia:

- **Morse não esconde nada.** É uma tabela pública: quem tem a tabela lê. Serve para transmitir, não para proteger.
- **Cifra inventada é o erro clássico.** A segurança tem de estar só na chave, nunca no segredo do método (princípio de Kerckhoffs). Método caseiro, mesmo misturando vários códigos, não tem a análise de anos que o Olm (vodozemac, auditado) tem, e cada camada a mais é código a mais para errar.

Onde o Morse pode entrar: como **meio**, levando a carta já cifrada pelo Olm. Por exemplo, som ou luz entre dois celulares sem Bluetooth. É muito lento: estimo poucos bytes por segundo, então minutos para a menor carta (97 bytes). Fica para a etapa do som, se fizer sentido lá.

## Passos

1. **Sonda medida (o próximo).** Um app de teste, montado só com `--dart-define=KRYPTOS_SONDA_BLE=true`, sem protocolo do Kryptos ainda:
   - um celular anuncia o serviço; o outro procura, conecta, pede o MTU e manda 66.594 bytes em pedaços;
   - quem recebe confere o SHA-256 dos bytes e responde;
   - mede, em linhas `MEDIDA`: tempo até achar, MTU aceito, tempo da transferência, se o SHA-256 conferiu;
   - repetir a 1, 5 e 10 metros (estimo o alcance; é o que a medida vai dizer).
   - Arquivos novos (planejado): `SondaBle.kt` e o canal `kryptos/sonda-ble`; permissões no manifesto; `-SondaBle` no `instalar-no-celular.ps1`.
2. **Carta de verdade:** a carta Olm passa pelo BLE e entra no `receber_carta`. Chave avulsa entregue no encontro (decisão 4).
3. **Quem é quem dentro da conexão:** como dois contatos se reconhecem sem anunciar identidade a quem está em volta. Desenho antes do código.
4. **Tela:** o modo Direto na Central, ao lado do relay.

## O que não resolve

- **Presença:** quem procura BLE por perto vê que há um Kryptos ali (o identificador de serviço é fixo). Não vê quem é.
- **Alcance e velocidade:** ainda não medidos (passo 1).
- **iPhone:** fora desta etapa.
- **Segundo plano:** fora da primeira versão (decisão 3).
