# Sessão Olm — conversa entre duas pessoas, passo 1 (só o núcleo)

Decidido em 07/11/2026. Código: `nucleo/src/sessao.rs` (protocolo) e `nucleo/src/aparelho.rs` (conversas da identidade aberta e regravação do cofre). Ainda não passa pela ponte nem aparece na tela.

## Para que serve

É o canal cifrado entre dois contatos que leram o QR um do outro. Usa a sessão Olm do vodozemac (Double Ratchet): cada mensagem tem chave própria, e a chave muda a cada troca de sentido da conversa.

A sessão fica só em memória, como a agenda. Quando o app fecha, a sessão some.

## Como abre

A escreve primeiro para B.

1. B gera uma **chave avulsa**: uma chave Curve25519 de uso único, assinada com a Ed25519 de B. A assinatura cobre `KRYPTOS/chave-avulsa/v1` seguido dos 32 bytes da chave, e assim não serve para mais nada que a mesma Ed25519 assinar.
2. A chave avulsa chega a A pelo servidor de repasse (decisão abaixo). O núcleo recebe e entrega a chave em bytes: `ChaveAvulsa::de_partes`, `chave_bytes` e `assinatura_bytes`.
3. A confere a assinatura com a Ed25519 de B lida no QR e abre a sessão (`Sessao::abrir`).
4. A primeira mensagem de A leva o que B precisa para abrir o lado dele. B abre com `Sessao::receber_primeira`, e só aceita se a mensagem veio da Curve25519 de A lida no QR.
5. Daí em diante, os dois lados usam `cifrar` e `decifrar`. As mensagens que A mandar antes de B responder também são "primeiras", mas B abre todas com a sessão que já abriu (`decifrar`). Se tentar abrir outra sessão com elas, a chave avulsa já foi gasta.

## Por onde a chave avulsa chega (decidido em 07/11/2026)

O caminho escolhido é o **servidor de repasse**, com um estoque de chaves avulsas assinadas e **sem chave reserva**. O QR continua na versão 1, só com a identidade. O modelo vem do Signal (X3DH: pré-chaves publicadas no servidor, entregues uma vez e apagadas), sem a pré-chave de médio prazo: uma mensagem feita com uma chave que não se gasta poderia ser repetida e reaparecer depois de queimada.

- Só o servidor cobre B fora do ar, o contato por código à distância e a reabertura de uma conversa perdida. O QR só cobre o primeiro encontro.
- O servidor não consegue trocar a chave: A confere a assinatura com a Ed25519 lida no QR.
- Pôr a chave no QR quase dobraria o texto e exigiria medir a leitura de novo.

Formato, rotas, limites e o que não resolve: `documentacao/arquitetura/estoque-de-chaves.md`.

## Chaves avulsas no cofre

Uma chave avulsa só vale se a parte secreta sobreviver ao app fechar, porque o servidor guarda a parte pública até alguém pedir. O `aparelho` grava a conta no cofre a cada mudança, sempre com o disco primeiro:

- **Gerar** (`nova_chave_avulsa`): gera, grava, lê de volta e só então entrega a chave. Sem cofre aberto, recusa (`COFRE_FECHADO`).
- **Gastar** (`receber`, primeira mensagem de uma conversa nova): abre a conversa, grava a conta com a chave gasta, lê de volta e só então devolve o texto.
  - Se a gravação falhar, a conversa é descartada e o texto não sai. Depois de reabrir o app, a mesma mensagem ainda abre, porque o cofre continua com a chave.
  - Sem isso, uma primeira mensagem repetida depois de reabrir o app abriria de novo e mostraria outra vez um texto que já foi lido.
- **Ler de volta:** o arquivo regravado tem de abrir com a mesma chave e trazer a mesma conta da memória, chaves avulsas incluídas (as contas são comparadas inteiras, em JSON).

`receber` também trata as primeiras mensagens que A manda antes de B responder: se a mensagem é da conversa já aberta (mesmo identificador), decifra nela, sem gastar outra chave.

Medido em 07/11/2026, no computador (Linux): o cofre vai de 640 bytes sem chave avulsa para 8.747 bytes com 50, e cada chave nova custa cerca de 1,1 ms em modo de distribuição (6,4 ms em debug), com gravação, `fsync` e releitura. No celular ainda não foi medido.

A conta do vodozemac 0.11.2 guarda até 5.000 chaves avulsas (100 × 50, no código dele). A seguinte descarta a mais antiga, e a primeira mensagem que usar a descartada é recusada. O teste `a_conta_descarta_a_chave_avulsa_mais_antiga_depois_do_maximo` confere esse limite. O estoque publicado vai ser bem menor; o número sai na etapa do servidor.

## Versão do Olm

É a versão 1, com MAC truncado em 8 bytes. A versão 2 do vodozemac usa o MAC inteiro, mas desde a 0.10 fica atrás do recurso `experimental-session-config`, e a regra 8.1 não aceita base experimental. Trocar de versão depois exige que os dois aparelhos troquem juntos: uma mensagem de outra versão é recusada (`SESSAO_VERSAO_DO_OLM`).

## Recusas

| Código | Quando |
|---|---|
| `SESSAO_ASSINATURA_INVALIDA` | A chave avulsa não foi assinada pela Ed25519 do contato, ou foi assinada sem o domínio do Kryptos |
| `SESSAO_CHAVE_FRACA` | Chave de ordem baixa na identidade, na chave avulsa, na chave base ou na catraca: o segredo sairia previsível. O vodozemac recusa desde a 0.10 (`NonContributoryKey`) |
| `SESSAO_REMETENTE_NAO_CONFERE` | A primeira mensagem não veio da chave do contato lida no QR |
| `SESSAO_CHAVE_AVULSA_DESCONHECIDA` | A primeira mensagem usa uma chave avulsa que este aparelho não tem: já gasta, gerada em outro aparelho ou perdida quando o app fechou |
| `SESSAO_VERSAO_DO_OLM` | A mensagem foi feita com outra versão do Olm |
| `SESSAO_NAO_E_PRIMEIRA` | Chegou uma mensagem comum onde se esperava a primeira |
| `SESSAO_ADULTERADA` | O MAC não confere: mensagem adulterada ou de outra sessão |
| `SESSAO_JA_USADA` | A chave desta mensagem já foi usada (mensagem repetida) |
| `SESSAO_SALTO_GRANDE` | Mensagens demais puladas na mesma cadeia |
| `SESSAO_FORMATO` | Bytes fora do formato de mensagem, chave ou assinatura |
| `SESSAO_CHAVE_AVULSA_NAO_GERADA` | O vodozemac não devolveu a chave que acabou de gerar |

Os erros do vodozemac trazem chaves públicas no texto. Aqui eles viram o código fixo, e a chave é descartada (regra 8.2). O `Debug` da sessão e da chave avulsa não mostra nada.

Mensagem recusada não gasta a chave avulsa: o vodozemac só a apaga depois de decifrar a primeira mensagem. Os testes provam isso, porque a mensagem certa ainda abre depois de todas as tentativas recusadas.

## Vetores conhecidos

- `ferramentas/oraculos/olm.py` monta a primeira mensagem de A para B pela especificação pública do Olm (3DH, HKDF, AES-256-CBC, HMAC-SHA256 truncado e protobuf escritos à mão), sem usar o vodozemac. Usa as chaves das RFC 7748 e 8032 e a conta fixa do cofre para B. Antes de gerar, calibra o X25519 pela RFC 7748 e o HKDF pela RFC 5869. O teste `vetor_conhecido_do_oraculo_abre` confere que o vodozemac abre essa mensagem e lê o texto `KRYPTOS vetor 1: A para B`.
- `ferramentas/oraculos/ordem_baixa.py` tem um X25519 próprio, escrito pela RFC 7748 e calibrado pelo vetor da seção 5.2. Com ele confere as 14 grafias de 32 bytes que zeram o X25519 (cinco valores de u, contando as formas não canônicas e o bit 255). Chaves comuns são o caso errado, e nenhuma delas zera. Os testes usam as 14 grafias nas quatro posições do acordo.

## Grafias equivalentes (achado de 07/11/2026)

O Olm confere o MAC sobre a mensagem **recodificada**, e não sobre os bytes que chegaram. Além disso, o X25519 ignora o bit 255 da chave. Por isso, alguns bits trocados mudam os bytes mas não mudam a mensagem: mesma chave, mesmo texto cifrado e mesmo MAC. O texto que chega é o original, e a chave daquela mensagem só serve uma vez. Mesmo assim:

- **O servidor de repasse não pode reconhecer mensagem repetida pelo hash dos bytes.** Duas grafias da mesma mensagem dariam hashes diferentes. A repetição é recusada no aparelho (`SESSAO_JA_USADA`).
- Medido: no vetor, 7 dos 1.472 bits trocados dão grafia equivalente. Numa mensagem comum de 63 bytes, foram sempre 4 de 504 em 300 conversas sorteadas.

## Como é provado

- **A cifra e B decifra:** 20 idas e voltas, texto vazio e texto de 64 KiB, mensagens fora de ordem, e várias primeiras mensagens antes da resposta.
- **Adulteração:** cada bit de cada byte, um por vez. No vetor (1.472 casos): 840 por MAC, 255 por chave avulsa desconhecida, 255 por remetente, 115 por formato e 7 grafias equivalentes. Na mensagem comum, em 5 conversas, nenhum bit trocado passou com conteúdo diferente. Além disso, mensagem repetida e mensagem de outra sessão são recusadas.
- **Ordem baixa:** as 14 grafias como identidade do contato, como chave avulsa assinada pelo próprio contato, como identidade na primeira mensagem, como chave base e como catraca. As 70 tentativas foram recusadas por `SESSAO_CHAVE_FRACA`. O caso certo é o mesmo caminho com chaves comuns, que abre.
- **Quem é quem:** chave avulsa assinada por outro, assinatura trocada e assinatura sem domínio. A primeira mensagem de C, que se passa por A, é recusada sem gastar a chave. Mensagem para outro aparelho também é recusada.
- **Calibração dos testes (07/11/2026):** três sabotagens no código, uma por vez, e cada uma derrubou pelo menos um teste. Foram tiradas a conferência da assinatura, a amarração do remetente ao contato e o domínio da assinatura.
- **Cofre (aparelho):**
  - a chave avulsa sobrevive a fechar e abrir o app;
  - a chave gasta continua gasta depois de reabrir;
  - a conversa não sobrevive a fechar o app;
  - com o cofre apagado com o app aberto, nenhuma chave é entregue e nenhum texto sai, e o cofre devolvido abre a mesma mensagem;
  - sem cofre não se gasta chave;
  - a varredura não acha chave avulsa secreta no cofre, e acha a mesma conta gravada em claro;
  - a releitura recusa o arquivo de outra conta.
- **Sabotagens do cofre (07/11/2026):** seis, uma por vez. Cinco foram pegas: receber sem gravar a chave gasta, gerar sem gravar, receber sem exigir cofre, a segunda primeira mensagem sem usar a conversa aberta e a releitura sem comparar. A sexta, regravar sem reler, fica isenta. O disco não devolve algo diferente do que acabou de ser gravado sem um gancho de falha no código de produção, e a conferência em si está calibrada pelo teste `releitura_recusa_cofre_com_outra_conta`.

## O que ainda não resolve

- A chamada ao relay a partir do app (ponte, rede e TLS).
- Ponte, tela e prova no aparelho (regra 5.5). Este passo prova o protocolo com as duas contas no mesmo processo, e não a troca entre os dois celulares.
- A conversa fica só em memória. Depois de reabrir o app, as mensagens comuns de uma conversa anterior são recusadas (`SEM_CONVERSA`). Gravar as conversas no cofre é assunto próprio.
- Com o cofre aberto, a chave do cofre fica na memória junto da conta, para regravar. Quem lê a memória do app já lê a conta, então isso acrescenta pouco, mas está registrado.
- A queima (1 minuto depois da leitura) e a mensagem só em memória na tela ainda não existem. O núcleo devolve o texto em `Zeroizing`, mas a cópia que a tela fizer é assunto da etapa da queima.
- O cartão do QR aceita qualquer Curve25519, inclusive de ordem baixa. Hoje a recusa acontece ao abrir a sessão. Recusar já na leitura do QR mexe no cartão e é assunto próprio.
