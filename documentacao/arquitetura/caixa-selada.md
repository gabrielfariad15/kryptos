# Caixa selada

## Para que serve

Guardar a mensagem recebida até a pessoa decidir ler, sem deixar o texto no disco, e depois queimá-la de verdade. Decisões de 07/11/2026:

- **Selar na chegada.** O núcleo abre a camada Olm só na memória e sela o texto com a chave de uma gaveta. O texto só abre quando a pessoa manda descriptografar.
- **Sem aviso de leitura e sem "digitando".** O relay não tem canal de volta para quem mandou.
- **Queima:** 1 minuto depois de lida. Na hora, ao sair do app ou ao apagar a tela.
- **Carta de conversa encerrada** (o app fechou e a conversa morreu): não abre, e a tela mostra só "N mensagens não puderam ser abertas". Exceção: a primeira mensagem de uma conversa cuja chave avulsa ainda não foi gasta abre mesmo depois de reabrir o app (sessao-olm.md).
- **Capacidade e limite:** 100 mensagens não queimadas, de até 65.536 bytes de texto cada (uns 60 mil caracteres em português, estimo).

## Arquivos

Mesma pasta do cofre (`noBackupFilesDir`). Todos existem desde a criação do cofre e têm sempre o mesmo tamanho: o disco não diz quantas mensagens há.

**Gavetas:** `gaveta-00.kryptos` até `gaveta-99.kryptos`, cada uma com 65.644 bytes (medido no teste `tamanhos_fixos`). São 6.564.400 bytes ao todo.

| Bytes | Campo |
|---|---|
| 24 | nonce, sorteado a cada selagem |
| 65.604 | XChaCha20-Poly1305 de: Ed25519 do remetente (32), Curve25519 do remetente (32), tamanho do texto (u32 LE), texto completado com zeros até 65.536 |
| 16 | etiqueta |

- **Chave:** `HKDF-SHA256(ikm = HMAC do Keystore sobre o pedido, info = "KRYPTOS/gaveta/v1")`.
- **Pedido:** `"KRYPTOS/gaveta/v1" || número da gaveta || nonce` (42 bytes).
- **Dado autenticado:** `"KRYPTOS/gaveta/v1" || número da gaveta`. Uma gaveta copiada para outro lugar não abre.
- **Gaveta vazia:** 65.644 bytes aleatórios. Não há cabeçalho fixo, então a vazia não se distingue da ocupada.

**Índice:** `caixa.kryptos`, 7.357 bytes (versão 2), cifrado com uma chave tirada da chave do cofre (`info = "KRYPTOS/caixa/v1"`), como a agenda:

- `"KRYCAIXA"`, versão (2), nonce (24);
- cifrado: próximo número (u64), seguido de 100 × (estado, número, resumo, remetente);
- **estado:** livre, reservada, não lida, lida ou queimada;
- **número:** ordem de chegada (não é data);
- **resumo:** SHA-256 da carta, para reconhecer a mesma carta entregue de novo.
- **remetente:** a Ed25519 de quem mandou, ou zeros quando não se sabe. Assim a Central mostra de quem são as não lidas sem abrir a gaveta (decisão de 07/11/2026). Quem consegue abrir o índice já precisa do PIN e do Keystore, e com eles veria a agenda inteira; o texto continua só na gaveta.
- **versão 1:** 4.157 bytes, sem remetente, das caixas criadas antes. Ainda é lida, com o remetente desconhecido. A gravação seguinte já sai na versão 2.

Leitura estrita: estado desconhecido, gaveta sem carta com número, resumo ou remetente, número fora da faixa ou repetido, tamanho que não é o da versão declarada, tudo isso é `CAIXA_CORROMPIDA`.

## Chaves no Keystore

Uma chave HMAC por gaveta, 100 ao todo, que existem sempre: alias `kryptos-gaveta-00` a `kryptos-gaveta-99`. O Kotlin é o mesmo do cofre (`ChaveDoAparelho.kt`): `criar` apaga a chave do alias, se houver, e gera outra, tentando o StrongBox e caindo no TEE. O app (`caixa_do_aparelho.dart`) cria as que faltam ao abrir (`prepararChaves`).

Queimar uma gaveta é sobrescrevê-la com bytes aleatórios e trocar a chave dela no hardware, com `criar` no mesmo alias. O número de chaves não muda, então o Keystore também não diz quantas mensagens há.

## Etapas

**Chegada** (`receber_carta`, a porta única, venha a carta do relay ou do modo Direto):

1. **Carta fora do formato:** erro `PROTOCOLO_*`. O app confirma ao relay e descarta.
2. **Destinatário diferente** deste aparelho: `NaoEParaMim` (o rádio ouve todo mundo).
3. **Resumo igual ao de uma carta guardada:** `JaGuardada`. O app confirma ao relay.
4. **Nenhuma gaveta livre:** `CaixaCheia`. O app não confirma, a carta fica no relay e a conversa não anda, porque a carta nem foi aberta.
5. **Decifrar:**
   - Primeira mensagem: o remetente é achado na agenda pela chave Curve25519 que vem na mensagem.
   - Mensagem comum: tenta cada conversa aberta. O vodozemac só muda a conversa quando a autenticação confere (lido no código da 0.11.2, `receiver_chain.rs`).
   - Não abriu, ou o texto passa de 65.536 bytes: `NaoAbre`. Entra na conta de `nao_abertas`, e o app confirma ao relay.
6. **Disco primeiro:** a gaveta é marcada como reservada no índice. Depois sai o pedido para o Keystore (`Selar`).

**Selagem** (`selar_chegada`, com o HMAC):

1. Sela, grava a gaveta e confere lendo de volta.
2. Abre de novo com o mesmo HMAC e compara.
3. Marca a gaveta como não lida no índice.

Só então o app confirma ao relay. Se algo falhar, a carta continua esperando: tenta de novo ou `desistir_da_chegada`.

**Leitura** (`pedido_de_leitura` → HMAC → `ler`): abre a gaveta, marca como lida no índice e só então devolve remetente e texto. O minuto começa a contar no relógio monotônico.

**Queima:**

- `queimar_vencidas`: o app chama a cada segundo e queima as lidas há 60 s ou mais.
- `queimar_lidas`: o app chama ao abrir, ao sair e ao apagar a tela. Queima na hora as lidas, as reservadas que nunca foram seladas e termina as queimas interrompidas.
- Cada queima segue esta ordem:
  1. marca a gaveta como queimada no índice (disco primeiro);
  2. sobrescreve a gaveta com bytes aleatórios e confere;
  3. o Kotlin troca a chave;
  4. `chave_trocada` libera a gaveta.
- App morto no meio do minuto: ao abrir, a lida queima na hora.

## Ponte e aparelho

- **Ponte** (`frontend/app/rust/src/api/nucleo.rs`):
  - `caixa_receber_carta`, `caixa_selar_chegada`, `caixa_desistir_da_chegada`;
  - `caixa_listar`, `caixa_nao_abertas`, `caixa_pedido_de_leitura`, `caixa_ler`;
  - `caixa_queimar_vencidas`, `caixa_queimar_lidas`, `caixa_chave_trocada`.
  - Para enviar: `cartao_proprio_texto`, `adicionar_contato_por_texto`, `nova_chave_avulsa`, `abrir_conversa` e `cifrar_carta`.
  - `procurar_texto` só devolve contagens, para o teste provar que o texto não ficou no disco.
- **Dart** (`lib/caixa_do_aparelho.dart`): faz o vai-e-volta do HMAC com o Keystore e troca a chave depois de cada queima. Se o Keystore ou o disco falharem na selagem, desiste da carta e a falha sobe.
- **No aparelho** (`integration_test/caixa_test.dart`): num celular só, B cria o cofre e as 100 chaves; A, outra identidade, cifra uma carta; B recebe, sela, lê e queima.
  - O texto é procurado em toda a pasta do app antes de ler e depois de queimar.
  - A busca é calibrada no próprio aparelho, gravando o texto em claro.
  - A linha MEDIDA traz os tempos de criar o cofre, criar as chaves, gravar a agenda, selar, ler, queimar e buscar.
- **Ponte, sem o celular:** o mesmo caminho no teste Rust `caixa_pela_ponte`, com um Keystore falso.

## Vetores conhecidos

O teste `vetores_conhecidos` confere o SHA-256 de uma gaveta e de um índice fixos:

- **gaveta:** `0aa74af231362518b38e1a503245b8c7199f9c65161267f8adee0546f2baa33e`
- **índice (versão 2):** `c556a24c4f402677ddf9d6f9a1b020df8c044bb580f32154e459c8f90e425af3`
- **índice (versão 1, ainda lido):** `9b3403a80ecb722593ac2585c1687edb0d613eb82bd6833a906ef18991721634`

Os mesmos valores saem do oráculo `ferramentas/oraculos/caixa.py`, sem o Rust. Ele usa o XChaCha20-Poly1305 do oráculo da agenda, que se calibra contra o rascunho draft-irtf-cfrg-xchacha-03.

## Como é provado

- **Formato (`caixa.rs`):**
  - tamanhos fixos;
  - índice: ida e volta; cada um dos 7.357 bytes trocado é recusado; 8 índices em claro fora das regras são recusados; o da versão 1 abre sem remetente e regravado vira versão 2;
  - gaveta:
    - ida e volta;
    - não abre com outra chave, nem em outra gaveta, nem vazia;
    - byte trocado (amostra de 67 posições) é recusado;
    - texto grande e HMAC de tamanho errado são recusados;
  - o texto e o remetente não aparecem em claro na gaveta;
  - vetores conhecidos.
- **Fluxo (`aparelho.rs`):**
  - do recebimento à queima: relay entregando de novo; leitura; 59 s não queima, 60 s queima; gaveta sobrescrita; com a chave trocada, nem os bytes antigos de volta abrem; a gaveta volta a servir;
  - mensagem comum achada pela conversa;
  - conversa encerrada: `NaoAbre` e a conta sobe;
  - lida antes de fechar queima ao abrir; não lida sobrevive e abre;
  - recusas antes de decifrar: outro destinatário, carta estragada, remetente fora da agenda, carta adulterada. Nenhuma anda a conversa;
  - chegada pendente bloqueia a próxima;
  - caixa cheia não abre a carta e a aceita depois;
  - falha de disco ao selar mantém a carta esperando;
  - texto de 65.536 bytes cabe e o núcleo recusa enviar um byte a mais;
  - índice adulterado não abre o cofre; cofre sem índice ganha caixa vazia.
- **Texto no disco:** antes de ler e depois da queima, o texto não aparece em nenhum arquivo da pasta. A busca é a mesma da varredura do cofre, em 8 formas. Ela é calibrada no mesmo teste: o texto gravado em claro é achado.
- **Calibração (07/11/2026):** sete sabotagens, uma por vez, e cada uma derrubou um teste:
  - decifrar antes de ver se há gaveta livre;
  - não sobrescrever na queima;
  - não gravar "lida" no disco;
  - queimar só depois de 60 s;
  - perder a carta quando a selagem falha;
  - não reconhecer carta repetida;
  - aceitar carta de outro destinatário.

## O que não resolve

- **O texto passa pela memória e pela tela.** Quem lê a memória do app (root, depurador) vê o texto enquanto ele está aberto. O Dart não garante que as cópias sumiram antes do coletor de lixo passar.
- **Chave do Keystore restaurada:** se o atacante já tinha root e uma cópia dos arquivos do Keystore antes da queima, ele pode restaurar a chave apagada e abrir a cópia antiga da gaveta. Só a *rollback resistance* impede isso, e o Android a esconde dos apps: o espaço dela fica reservado para chaves do sistema (`KeyProtection.Builder.setRollbackResistant` é `@hide`, conferido no código do AOSP em 07/11/2026). Nenhum app comum tem essa garantia, em aparelho nenhum.
- **Índice antigo no armazenamento:** a troca de nome deixa versões antigas do índice em blocos livres. Com a chave do cofre (PIN + Keystore), elas mostram quantas mensagens havia e em que estado, nunca o texto nem o remetente.
- **App morto entre decifrar e reservar a gaveta:** a carta se perde. A conversa já andou, e o relay, se entregar de novo, recebe `NaoAbre`.
- **Gaveta apagada à mão:** a leitura dela falha, e a queima a recria. O índice não confere a existência das gavetas ao abrir.
- **Ainda não feito:** a tela da caixa e a queima ligada ao ciclo de vida do app (a cada segundo, ao sair e ao apagar a tela). `CaixaDoAparelho` já tem as chamadas; falta a tela que as faz.
