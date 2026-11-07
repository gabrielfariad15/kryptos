# Estoque de chaves avulsas no relay

Decidido em 07/11/2026. Código: `protocolo/src/lib.rs` (formato, usado pelo app e pelo relay), `backend/relay/src/estoque.rs` (estoque), `backend/relay/src/lib.rs` (rotas), `nucleo/src/sessao.rs` e `nucleo/src/aparelho.rs` (lado do app).

## Para que serve

A só abre uma conversa com B se tiver uma chave avulsa de B (`sessao-olm.md`). B publica um estoque dessas chaves no relay, e A retira uma quando vai escrever pela primeira vez, mesmo com B fora do ar. O relay não consegue trocar a chave: cada uma vem assinada pela Ed25519 de B, e A confere a assinatura contra a chave que leu no QR.

## Decisões

- **Sem chave reserva.** No Matrix, uma mensagem feita com a chave reserva pode ser repetida enquanto a reserva estiver ativa. No Kryptos nenhuma mensagem pode reaparecer depois de queimada. Com o estoque vazio, A recebe `SEM_CHAVE` e tenta depois. As conversas já abertas não dependem do estoque.
- **Estoque só em memória.** Se o relay reiniciar, o estoque se perde, e cada aparelho repõe o seu ao abrir o app. O disco entra junto com a fila de mensagens.
- **O relay não lê o IP de ninguém.**
  - No relay fica o limite por destinatário.
  - O limite por endereço de rede, contra enxurrada, fica no proxy da implantação (Caddy ou nginx), em memória e numa janela curta.
  - Com o roteamento em camadas, o relay só verá o IP do último servidor da rota, e só o limite por destinatário continua valendo.
  - Guardar ou não registro de acesso no proxy depende da avaliação do Marco Civil, art. 15 (pendência jurídica).
- **Contador em vez de relógio.** A publicação traz um número que só sobe. O relay recusa número igual ou menor que o último aceito e devolve o último, para o aparelho continuar dali sem precisar guardar nada. Uma publicação copiada não pode ser repetida, e o relógio errado do aparelho não atrapalha (regra 7.2).

## Formato (versão 1)

```
corpo      = versão (1) || dono (32, Ed25519) || número (8, u64 LE) || n (2, u16 LE)
             || n × (chave Curve25519 (32) || assinatura da chave (64))
publicação = corpo || assinatura (64) = Ed25519 do dono sobre "KRYPTOS/publicacao/v1" || corpo
retirada   = chave (32) || assinatura da chave (64)
```

- `n` vai de 0 a 100. Com 0, a publicação só consulta o estoque.
- A leitura é estrita: tamanho exato e nenhum byte a mais.
- A assinatura de cada chave é a da `sessao` (`KRYPTOS/chave-avulsa/v1`). O relay não a confere; quem retira, sim.
- O dono no caminho do pedido vai em 64 caracteres hexadecimais minúsculos, e só essa grafia é aceita.

## Rotas

| Pedido | Resposta |
|---|---|
| `POST /v1/publicar` (corpo: publicação) | 200 `ESTOQUE <n>` |
| | 400 `PROTOCOLO_*` (formato ou assinatura) |
| | 409 `NUMERO_VELHO <último>` |
| | 409 `ESTOQUE_CHEIO <n>` |
| | 409 `CHAVE_REPETIDA` |
| | 413 (corpo maior que 9.707 bytes) |
| | 503 `RELAY_CHEIO` |
| `POST /v1/retirar/{dono}` | 200 com os 96 bytes da chave |
| | 400 `PROTOCOLO_DONO` |
| | 404 `SEM_CHAVE` |
| | 429 `LIMITE`, com `Retry-After` em segundos |

- **Publicar soma, nunca substitui.** Uma chave já retirada não pode voltar ao estoque e ser entregue a outra pessoa.
- **Publicação recusada não muda nada,** nem o último número.
- **Retirar entrega a chave mais antiga** e a apaga.
- **Dono desconhecido e estoque vazio dão a mesma resposta.** O relay não conta a quem pergunta se alguém publicou.
- **Limite por destinatário:** 10 retiradas seguidas, depois uma a cada 6 minutos, sem acumular mais de 10. Conta pelo relógio monotônico do servidor.

Números ainda sem medida (estimo):
- 100 chaves por dono;
- 20.000 donos por relay, cerca de 192 MB no pior caso;
- 10 retiradas seguidas;
- uma ficha a cada 6 minutos.

## Lado do app

- `aparelho::preparar_publicacao(quantas, numero)` gera as chaves, grava a conta no cofre, lê de volta e só então devolve a publicação assinada. Sem cofre aberto, recusa.
- `ChaveAvulsa::da_retirada(bytes)` lê a resposta da retirada. A assinatura é conferida em `Sessao::abrir`, contra o contato lido no QR.

## Vetores conhecidos

`ferramentas/oraculos/publicacao.py` monta e assina duas publicações fora do Rust, com a Ed25519 do teste 1 da RFC 8032 (calibrada pela assinatura da mensagem vazia da mesma RFC): uma com 2 chaves e número 7, e uma consulta com o número máximo. Os vetores ficam em `protocolo/src/vetor_publicacao.hex` e `vetor_consulta.hex`, e o Rust confere a assinatura e a ida e volta.

## Como é provado

- **Protocolo:**
  - os vetores conhecidos;
  - cada bit dos 299 bytes trocado é recusado;
  - publicação cortada em cada posição, ou com um byte a mais, é recusada;
  - mais de 100 chaves é recusado ao montar e ao ler;
  - a assinatura de uma publicação não serve para outra com outro número;
  - o dono em texto é estrito;
  - o `Debug` não mostra chave.
- **Estoque, com o relógio passado pelo teste:**
  - soma em ordem e retira uma vez só;
  - número velho é recusado e o último é devolvido;
  - estoque cheio recusa a publicação inteira;
  - chave repetida é recusada;
  - dono desconhecido e estoque vazio respondem igual;
  - donos demais são recusados;
  - o limite segue o relógio, não acumula e não passa de um dono para outro.
- **HTTP, com o núcleo de verdade dos dois lados:**
  - B publica, A retira, A abre a conversa e B lê;
  - cada chave sai uma vez;
  - publicação repetida é recusada;
  - adulterada, vazia ou grande demais é recusada, e nenhuma recusa põe nada no estoque;
  - o limite responde 429 com espera;
  - dono fora do formato e método errado são recusados.
- **Sabotagens (07/11/2026):** nove, uma por vez, e todas derrubaram pelo menos um teste:
  - retirar sem apagar;
  - publicar substituindo;
  - não conferir o número;
  - não limitar;
  - aceitar chave repetida;
  - não ter teto de estoque;
  - não conferir a assinatura no protocolo;
  - assinar sem o domínio;
  - o relay aceitar sem conferir a assinatura.

## O que não resolve

- **O relay sabe quem pede a chave de quem.** Ele vê o pedido de retirada pelo dono, mas não sabe quem pediu, porque não lê o IP. O proxy vê o IP. Até o roteamento em camadas, os dois juntos sabem.
- **Esgotar o estoque de alguém de propósito** ainda é possível, com muitas origens de rede e paciência. O limite por destinatário só atrasa. Com o estoque vazio, as conversas novas com aquela pessoa esperam a reposição.
- **O ritmo da reposição denuncia quando o aparelho está ligado:** se nenhuma chave nova aparece, o aparelho provavelmente está desligado. Um estudo de 2025 sobre o WhatsApp mostrou isso. Repor numa hora fixa, e não na hora em que o app abre, diminui o problema; fica para a etapa do app.
- **Relay reiniciado perde o estoque** até os aparelhos reporem.
- **Ainda não há transporte de verdade:** nada de TLS, implantação ou chamada a partir do app. A ponte e a tela são etapa própria.
