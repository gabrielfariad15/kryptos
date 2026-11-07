# Malote

## Para que serve

Guardar no relay as cartas de quem está sem internet ou com o app fechado, até o destinatário buscar. É a fila que faltava para duas pessoas conversarem pela internet (decisões de 07/11/2026: armazenamento em disco com redb, busca assinada só pelo destinatário, apagar depois da confirmação, remetente anônimo, validade de 7 dias).

## Rotas

**`POST /v1/depositar`.** O corpo é a própria carta (`kryptos_protocolo::carta`). Respostas:

- `200 GUARDADA`;
- `400 PROTOCOLO_*`: carta fora do formato;
- `409 MALOTE_CHEIO`: o destinatário já tem cartas ou bytes demais esperando;
- `503 RELAY_CHEIO`;
- `413`: corpo acima da maior carta (66.594 bytes).

Ninguém se identifica para depositar. A carta não diz quem mandou, e o relay não lê o IP.

**`POST /v1/buscar`.** O corpo é a busca assinada pelo destinatário:

```text
corpo = versão (1) || destinatário (32, Ed25519) || número (8, u64 LE)
        || n (2, u16 LE) || n × id (16)
busca = corpo || assinatura (64) = Ed25519 do destinatário sobre "KRYPTOS/busca/v1" || corpo
```

- Os ids confirmam as cartas da entrega anterior que já foram seladas no aparelho. O relay as apaga antes de entregar as próximas: confirmar e buscar é um pedido só.
- O número só sobe, como na publicação: uma busca copiada e reenviada é recusada com `409 NUMERO_VELHO <último>`. Não depende de relógio.
- Até 500 ids por busca, sem repetir. Leitura estrita: tamanho exato.

Respostas:

- `200`: a entrega, `n (2) || n × (id (16) || tamanho (4, u32 LE) || carta)`, na ordem de chegada, até 1 MiB. O aparelho busca de novo até vir vazia.
- `403 PROTOCOLO_ASSINATURA`: assinatura que não é do destinatário.
- `400 PROTOCOLO_*`: busca fora do formato.

Carta não confirmada é entregue de novo na busca seguinte. Assim, uma resposta perdida na rede não perde carta.

## Armazenamento

redb 4.2.0, num arquivo dado por `KRYPTOS_RELAY_MALOTE`. Sem a variável, o malote fica só na memória, e o relay avisa no log que as cartas somem se ele reiniciar.

| Tabela | Chave | Valor |
|---|---|---|
| cartas | destinatário (32) ‖ sequência (8, BE) | id (16) ‖ depósito (8, segundos Unix) ‖ carta |
| destinos | destinatário (32) | último número (8) ‖ cartas (4) ‖ bytes (8) ‖ mexido (8) |
| meta | `sequencia`, `bytes` | u64 |

- A sequência é do relay inteiro e só sobe, o que dá a ordem de chegada. O id é sorteado, para quem busca não deduzir quantas cartas o relay recebeu.
- O banco grava fora da thread do servidor (`spawn_blocking`).
- A validade usa o relógio de parede, porque precisa valer depois de um reinício.
- A cada 10 minutos, uma varredura apaga as cartas vencidas e os destinatários sem carta parados há 7 dias.

Busca de quem nunca recebeu carta devolve uma entrega vazia e não grava nada. Senão, qualquer um, gerando chaves à vontade, encheria a tabela de destinos.

## Limites (estimo)

- 500 cartas e 8 MiB por destinatário;
- 20.000 destinatários;
- 2 GiB de cartas no relay inteiro;
- 7 dias de validade.

Os limites ficam em `Limites::PADRAO`. Os testes usam limites menores.

## Como é provado

- **Formato (`protocolo/src/malote.rs`):**
  - vetor conhecido assinado pelo oráculo `ferramentas/oraculos/malote.py`, que se calibra contra o teste 1 da RFC 8032;
  - cada bit trocado do vetor é recusado;
  - tamanhos errados, versão errada, id repetido e ids demais são recusados;
  - entrega: ida e volta; cortada, com sobra, com `n` maior e com id repetido é recusada.
- **Malote (`backend/relay/src/malote.rs`):**
  - entrega na ordem; sem confirmar volta igual; confirmada some; outro destinatário não é tocado;
  - número que não sobe é recusado sem apagar nada;
  - quem nunca recebeu não grava registro;
  - carta vence em 7 dias exatos;
  - a varredura tira vencidas e destinatários parados;
  - limites por destinatário e do relay; confirmar libera lugar;
  - entrega até 1 MiB;
  - o arquivo sobrevive a um reinício, inclusive o número da última busca.
- **HTTP (`backend/relay/tests/malote_http.rs`), com o núcleo de verdade:**
  - B publica; A retira a chave e deposita duas cartas;
  - um intruso com outra chave não consegue buscar;
  - B busca, decifra as duas e confirma;
  - a busca reenviada é recusada;
  - fora do formato e corpo grande demais são recusados;
  - a maior carta passa.
- **Calibração (07/11/2026):** sete sabotagens, uma por vez, e cada uma derrubou um teste:
  - aceitar número igual;
  - apagar sem confirmação;
  - vencer só depois dos 7 dias;
  - tirar o limite de cartas;
  - tirar o limite de corpo da rota de depósito;
  - não cortar a entrega em 1 MiB;
  - gravar registro na busca de quem não tem carta. Esta só foi pega depois de o teste ser reforçado.

## O que não resolve

- **Metadados no relay:** o relay sabe para quem há cartas, quantas, de que tamanho e quando chegaram. Não sabe de quem são.
- **Enchimento por estranho:** qualquer um pode encher o malote de um destinatário até o limite (500 cartas ou 8 MiB). O limite por IP fica no proxy da implantação. As cartas de lixo não abrem no aparelho e entram na conta de "não puderam ser abertas".
- **Relógio do servidor:** se o relógio do relay andar para trás, as cartas duram mais. Se andar para frente, vencem antes.
- **Disco do relay:** o arquivo do redb guarda as cartas cifradas até a confirmação ou os 7 dias. Apagar de um disco não é garantido, mas a carta é cifrada de ponta a ponta e a conversa tem sigilo para a frente.
- **Ainda não feito:** o app chamando estas rotas (próxima etapa).
