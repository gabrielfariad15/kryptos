# Transportes: por onde a mensagem viaja

Decidido em 07/11/2026. Código: `protocolo/src/carta.rs`.

## A exigência

O Kryptos funciona **com e sem internet** (`DECISOES-FASE-0.md`, seção 8):

- **Internet:** relay, e depois o roteamento em camadas.
- **Perto, sem internet:** Bluetooth e Wi-Fi Direct.
- **Som:** só para trocar o código de contato.
- **Local:** mensagem que só abre num raio definido. É uma condição de abertura, não um caminho.
- **Longa distância, sem internet:** rádio LoRa ligado ao celular por Bluetooth, no modelo do Meshtastic.
- **Cubo:** estima a distância juntando Bluetooth, Wi-Fi, GPS e som.

O modo Direto é o item 7 da ordem de construção, mas a exigência vale desde já: nada no formato da mensagem nem no núcleo pode depender da internet. Em 07/11/2026 a fila do relay quase foi desenhada só para a internet; este documento existe para isso não se repetir.

## A carta

A mensagem cifrada viaja como **carta**, igual em qualquer caminho:

```
carta = versão (1) || destinatário (32, Ed25519) || tipo (1) || mensagem Olm (resto)
  tipo: 0 = primeira mensagem da conversa, 1 = mensagem comum
```

- **O núcleo recebe cartas por uma porta só** e não sabe por onde elas vieram.
- **Quem manda não vai na carta.** Nem o relay nem quem escuta o rádio ficam sabendo. O destinatário descobre ao decifrar:
  - na primeira mensagem, pela chave de identidade que vem dentro dela, conferida contra a agenda;
  - nas comuns, testando as conversas abertas. Uma tentativa recusada não estraga a conversa (o teste `mensagem_comum_com_qualquer_bit_trocado_e_recusada` prova isso).
- **Não há campo de tamanho:** a carta é a sequência inteira de bytes. Dividir em pedaços e remontar é tarefa do transporte.

## O que todo transporte tem de garantir, e o que não precisa

- **Tem de** entregar a carta inteira ao núcleo, ou não entregar.
- **Não precisa** garantir ordem: o Olm aceita mensagens fora de ordem.
- **Não precisa** evitar duplicata: a mesma carta pode chegar por dois caminhos, ou duas vezes pelo mesmo. Quem recusa a repetida é o núcleo (`SESSAO_JA_USADA`), e nunca pela comparação dos bytes: o Olm aceita grafias diferentes da mesma mensagem (`sessao-olm.md`).

## Tamanho medido

Medido em 07/11/2026 com mensagens Olm de verdade (vodozemac 0.11.2), e protegido pelo teste `tamanho_da_carta_medido`:

| Caracteres do texto | Carta da primeira mensagem | Carta de mensagem comum |
|---|---|---|
| 0 a 15 | 202 bytes | 97 bytes |
| 16 a 31 | 218 | 113 |
| 32 | 234 | 129 |
| 100 | 299 | 193 |
| 140 | 332 | 226 |
| 200 | 396 | 290 |

Um pacote de rádio Meshtastic leva cerca de 233 bytes de dados (`Data.payload`). Então:

- A primeira mensagem cabe num pacote até 31 caracteres. A comum cabe pelo menos até 140.
- **O rádio vai precisar dividir a carta em pedaços** e remontar do outro lado.
- O destinatário ocupa 32 bytes da carta. No rádio, pode viajar abreviado e voltar inteiro antes de entregar ao núcleo. Isso fica para a etapa do rádio.
- A velocidade do LoRa é de poucos bytes por segundo (estimo, pelos relatos do Meshtastic), então texto longo por rádio vai ser lento.

## Chave avulsa sem internet

Para abrir uma conversa, A precisa de uma chave avulsa de B (`sessao-olm.md`):

- **Com internet,** ela vem do estoque do relay (`estoque-de-chaves.md`).
- **Sem internet,** ela tem de passar no encontro: por Bluetooth, logo depois da leitura do QR, ou por outro caminho do modo Direto. O formato de 96 bytes é o mesmo da retirada no relay (`ChaveAvulsa::da_retirada`).

## Ordem

1. **Feito:** a carta, com o tamanho medido.
2. Núcleo: receber a carta por uma porta só, caixa de entrada e queima de 1 minuto depois da leitura.
3. Relay: a fila de cartas (internet).
4. Ponte e tela, com a internet primeiro.
5. Modo Direto: Bluetooth e Wi-Fi Direct, som, rádio e Cubo, usando a mesma carta e o mesmo núcleo.

## O que não resolve

- **Metadados no rádio:** quem escuta o rádio vê o destinatário (ou a abreviação dele), o tamanho e a hora, como no relay.
- **Dividir em pedaços e repetir o envio no rádio** ainda não existe.
