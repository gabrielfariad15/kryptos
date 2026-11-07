# Agenda gravada

## Para que serve

Os contatos voltam quando o app abre de novo. Decisão de 07/11/2026, que substituiu a agenda só em memória: sem ela, cada contato teria de ser conferido por QR, pessoalmente, toda vez que o app fechasse.

Só a agenda vai para o disco. Conversas (sessões Olm) e mensagens continuam fora dele: fechar o app encerra todas as conversas.

## O que guarda e o que não guarda

Guarda, de cada contato:

- as duas chaves públicas do cartão (Ed25519 e Curve25519);
- como a chave foi conferida (hoje, só "por QR, pessoalmente");
- lugar reservado para o apelido, que você dá e que nunca sai do aparelho. A tela do apelido ainda não existe, então o lugar fica vazio.

Não guarda: data, último contato, ordem por conversa recente nem contagem de mensagens. A ordem do arquivo é a ordem em que os contatos entraram.

## Arquivo

Pasta: a mesma do cofre (`noBackupFilesDir`). Nome: `agenda.kryptos`.

| Bytes | Campo |
|---|---|
| 8 | `KRYAGEND` |
| 1 | versão (1) |
| 24 | nonce da gravação, sorteado |
| 490.002 | agenda cifrada (XChaCha20-Poly1305; o cabeçalho entra como dado autenticado) |
| 16 | etiqueta de autenticação |

Total: 490.051 bytes, sempre (medido no teste `tamanho_e_o_mesmo_vazia_e_cheia`).

A agenda em claro tem 2 bytes de quantidade (u16 little-endian) e 5.000 lugares de 98 bytes:

| Bytes | Campo |
|---|---|
| 1 | conferência (1 = por QR) |
| 32 | Ed25519 |
| 32 | Curve25519 |
| 1 | tamanho do apelido (0 por enquanto) |
| 32 | apelido (zeros por enquanto) |

Os lugares sem contato ficam em zero. Por isso o arquivo tem o mesmo tamanho com 0 ou com 5.000 contatos: o tamanho não diz quantos contatos você tem. O arquivo é criado junto com o cofre, mesmo vazio: a existência dele também não diz nada.

Chave: `HKDF-SHA256(sal vazio, ikm = chave do cofre, info = "KRYPTOS/agenda/v1")`. A chave do cofre é a que volta do Keystore (PIN + hardware). O `info` é outro que o da conta, então a chave da agenda nunca é a da conta.

Nonce de 24 bytes sorteado a cada gravação. Com esse tamanho, sortear não tem risco prático de repetir, mesmo com a chave fixa.

## Leitura estrita

Tudo isto é `AGENDA_CORROMPIDA`:

- tamanho diferente de 490.051;
- etiqueta que não confere (chave errada, qualquer byte trocado);
- quantidade acima de 5.000;
- lugar vazio com algum byte fora de zero;
- conferência desconhecida ou apelido preenchido;
- chave Ed25519 fora da curva;
- contato repetido, em conflito com outro (mesmo código ou uma das chaves igual) ou igual ao próprio aparelho.

Cada contato do arquivo passa pelas mesmas regras de quando entrou pelo QR. Versão diferente de 1 é `AGENDA_VERSAO_DESCONHECIDA`.

## Etapas

- **Criar o cofre:** a agenda da memória (vazia, ou com quem já foi lido antes) é gravada e lida de volta. Só então a identidade é gravada.
- **Abrir o cofre:** depois de a identidade abrir, a agenda é aberta com a mesma chave. Se não abrir, a abertura falha com `AGENDA_CORROMPIDA`: nada vai para a memória e nenhum arquivo é apagado. Sem arquivo (cofre criado antes desta etapa), a agenda começa vazia e já é gravada.
- **Contato novo, com o cofre aberto:** o disco vem primeiro. A agenda nova é cifrada, gravada (temporário, `fsync`, troca de nome), lida de volta e comparada. Só então troca a da memória. Se a gravação falhar, o contato não entra. "Já estava" não regrava.
- **Fechar o app:** a agenda sai da memória, e o arquivo fica.
- **Apagar:** `cofre::apagar` leva a agenda junto com o cofre e o contador.

Com 5.000 lugares, cada contato novo regrava cerca de 490 KB. O tempo no celular ainda não foi medido. Entra no teste com os dois aparelhos.

## Vetor conhecido

O teste `vetor_conhecido` grava dois contatos fixos com chave e nonce fixos e confere o SHA-256 do arquivo:

`dc55e7553d9e5e75c22d37b8549524804807ad0ab4ecc92b565243cfc3bcbdd5`

O mesmo valor sai do oráculo `ferramentas/oraculos/agenda.py`, que não usa o Rust. O oráculo usa o `cryptography` do Python e um HChaCha20 escrito à mão. Antes de montar o vetor, ele se calibra contra o HChaCha20 (seção 2.2.1) e contra a etiqueta do XChaCha20-Poly1305 (seção A.3.1) do rascunho draft-irtf-cfrg-xchacha-03. Sabotagem do oráculo (07/11/2026): com uma rodada a menos no HChaCha20, a calibração falha; com a conferência trocada de 1 para 2, sai outro SHA-256.

## Como é provado

- **Núcleo (`agenda_gravada.rs`):**
  - ida e volta;
  - mesmo tamanho vazia e cheia;
  - agenda cheia recusa contato novo e reconhece quem já está;
  - nonce novo a cada gravação;
  - chave errada não abre;
  - byte trocado em 461 posições (cabeçalho inteiro, começo, meio e fim) é recusado;
  - tamanhos errados são recusados;
  - 13 agendas em claro fora das regras, cifradas com a chave certa, são recusadas;
  - o arquivo não contém pedaço das chaves dos contatos em claro;
  - vetor conhecido.
- **Aparelho (`aparelho.rs`):**
  - o contato lido antes do cofre vai junto na criação;
  - contato novo regrava com o mesmo tamanho;
  - "já estava" não regrava;
  - fechar e abrir traz os contatos na mesma ordem;
  - apagar leva a agenda;
  - cofre sem agenda abre com agenda vazia, já gravada;
  - agenda adulterada não abre, não apaga nada e não conta como PIN errado;
  - gravação que falha não põe o contato na memória nem deixa temporário.
- **Calibração (07/11/2026):** seis sabotagens, uma por vez, e cada uma derrubou o teste dela:
  - aceitar lugar vazio sujo;
  - ignorar a falha de gravação;
  - abrir agenda adulterada como vazia;
  - deixar entrar o contato 5.001;
  - gravar sem os lugares vazios;
  - criar o cofre sem a agenda.

## O que não resolve

- **Coação:** quem obrigar você a desbloquear o app vê a agenda.
- **A agenda é o mapa de com quem você fala.** Cifrada e sem datas, mas existe. Senha de pânico é outra etapa.
- **Resto no armazenamento:** a troca de nome deixa a versão anterior do arquivo em blocos livres do armazenamento, até serem reaproveitados. Sem a chave do Keystore ela não abre, e o "apagar tudo" apaga essa chave (Kotlin). Apagar a chave a cada gravação não está feito.
- **Transferência entre aparelhos (Android 12+):** `android:allowBackup="false"` não impede a transferência direta de um aparelho para outro. A pasta `noBackupFilesDir` fica fora do backup automático, e as chaves do Keystore não saem do aparelho, então o que fosse copiado não abriria. Isso não foi medido no aparelho. Uma regra de extração (`dataExtractionRules`) que exclua tudo entra na próxima etapa do app, como segunda barreira.
- **Agenda corrompida trava a abertura.** A tela mostra `AGENDA_CORROMPIDA`, mas ainda não há saída para recomeçar só com a agenda vazia. A decisão fica para o "apagar tudo", junto do `CHAVE_AUSENTE`.
- **Apelido:** o lugar existe, mas a tela ainda não.
