# Telas de conversa

## Para que serve

Mostrar os canais e as mensagens sem deixar texto à vista além do necessário. São duas telas, com o visual aprovado em 07/11/2026 (U1Central e U2Conversa):

- **Modo 01 · Central:** os canais e as novas de cada um.
- **Modo 02 · Conversa:** as mensagens de um canal e o campo para escrever.

Decisões de 07/11/2026:

- **Central:** cada canal mostra "1 NOVA" ou "N NOVAS", a partir do remetente guardado no índice da caixa (caixa-selada.md). Tocar no canal abre a conversa.
- **Remetente fora da agenda, caixa antiga sem remetente:** viram canais só de leitura ("FORA DA AGENDA", "SEM REMETENTE").
- **Segurar para ler:**
  - a primeira vez decifra e começa o minuto até a queima;
  - o texto só aparece enquanto o dedo está na mensagem;
  - fechada, a mensagem mostra sempre 8 grupos de letras, para o tamanho não aparecer. As letras são enfeite, sorteadas com o gerador seguro uma vez por mensagem a cada abertura da conversa: não vêm do texto nem da posição da mensagem, então duas mensagens, ou a mesma em dois aparelhos, não mostram as mesmas letras (antes repetiam, achado no teste de 07/11/2026).
  - a hora "RX" é a de chegada gravada no índice da caixa (caixa-selada.md), a mesma depois de reabrir o app. Mensagem de uma caixa antiga, sem hora gravada, mostra "RX --:--" e vem antes das outras: a tela não inventa hora (antes mostrava a hora em que a viu, achado no teste de 08/11/2026).
- **Enviadas:** só na memória, fechadas como as recebidas. Queimam 60 s depois de enviadas e somem quando o app sai da frente.
- **Avisos:** a Central mostra "N mensagens não puderam ser abertas", a caixa cheia e a falha do relay ou da caixa, sempre com o código.
- **Modos:** Grupo e Direto aparecem apagados, com "PRÓXIMA FASE".
- **Tela segura:** FLAG_SECURE no app inteiro.

## Segurar

O gesto é o de segurar do Flutter (meio segundo, `kLongPressTimeout`): um toque rápido ou a rolagem da lista não abrem a mensagem nem começam a queima.

Um `Listener` em volta vê o dedo sair mesmo depois de o gesto ter sido aceito, porque o `GestureDetector` não avisa o cancelamento nesse caso.

Os bytes decifrados ficam na tela só enquanto o dedo está na mensagem e são zerados ao soltar. Se o dedo sai antes de o núcleo responder, o texto que chega é zerado sem aparecer. A mensagem já lida volta a ser decifrada da gaveta a cada vez que é segurada; o núcleo aceita a leitura de uma gaveta lida até a queima.

## Ciclo de vida

O plantão (`lib/plantao.dart`) escuta o ciclo de vida do app pelo `AppLifecycleListener`. No Android, o motor do Flutter passa pelos estados nesta ordem: resumed, inactive, hidden e paused.

| Momento | Estado | O que acontece |
|---|---|---|
| Cofre aberto, app de volta à frente | resumed | Termina as queimas interrompidas, prepara as 100 chaves das gavetas, repõe as chaves avulsas no relay e busca as cartas |
| App na frente | resumed | Busca no relay a cada 10 s e confere a queima a cada 1 s |
| Algo por cima (aviso do sistema, cortina de notificações, lista de recentes) | inactive | Esconde o texto aberto |
| App no fundo, tela apagada | hidden e paused | Para a busca, queima na hora as lidas e apaga da memória as enviadas, com os bytes zerados |

Uma operação de cada vez no núcleo: a chegada de uma carta tem duas etapas (reservar a gaveta e selar com o Keystore), e uma leitura no meio delas seria recusada. O plantão põe busca, leitura, queima e envio numa fila.

A contagem na tela ("QUEIMA EM 00:42") é feita pelo app a partir do momento da primeira leitura. Quem queima é o núcleo, com o relógio dele (`queimar_vencidas`), então a tela e a queima podem diferir de alguns décimos de segundo (estimo).

## Relay

O endereço é fixado na montagem do app, com `--dart-define=KRYPTOS_RELAY=<endereço>`:

- **Com endereço:** http só vale para a própria máquina (o relay de teste, pelo `adb reverse`); fora dela, só https (relay.dart).
- **Sem endereço:** o app só lê e queima o que já está na caixa. A Central mostra "SEM RELAY" e a conversa não tem campo de texto.

## Teclado

O campo de texto desliga a sugestão, a correção e o aprendizado do teclado (`enableIMEPersonalizedLearning: false`). No Android, isso pede ao teclado que não guarde o que foi digitado no dicionário dele (`IME_FLAG_NO_PERSONALIZED_LEARNING`). É um pedido: o teclado de terceiros pode ignorar.

## O que não resolve

- **Strings do Dart:** o texto aberto vira uma string do Dart para ser desenhado, e o texto digitado fica no controlador do campo. Strings do Dart não podem ser zeradas. Ficam na memória do app até o coletor de lixo passar.
- **Câmera apontada para a tela:** a tela segura bloqueia o print, a gravação e a miniatura nos recentes, mas não uma câmera apontada para o celular.
- **Leitor de tela:** segurar não tem equivalente acessível para quem usa leitor de tela. Fica para depois.
- **Conversa encerrada do outro lado:** se o app de quem recebe fechou, a conversa dele morreu. Cartas comuns da conversa antiga não abrem e entram no aviso "não puderam ser abertas". Quem mandou não fica sabendo. A primeira mensagem de uma conversa cuja chave avulsa ainda não foi gasta abre mesmo assim (sessao-olm.md): foi o que aconteceu no passo 8 do teste de 07/11/2026.
- **Conferência de chave na Central:** o bloco do visual aprovado (os 12 grupos para comparar pessoalmente) ainda não existe. O canal mostra só "CHAVE NÃO CONFERIDA" em laranja para quem entrou sem QR.

## Teste com os dois celulares

Os dois celulares no cabo do mesmo computador. Uma linha:

```
powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -Relay
```

O script sobe o relay de teste, instala o app nos dois e fica esperando você digitar FIM (Enter sozinho não encerra: o Enter que sobra do bloco colado no console encerrava a espera antes do teste). Depois do FIM, desliga o relay e mostra as linhas do app no logcat de cada celular (só códigos, regra 8.2).

Para filmar ou capturar a tela durante o teste, acrescente `-TelaLivre`. O app sai sem FLAG_SECURE, com uma faixa laranja "TELA LIBERADA · SÓ TESTE" em todas as telas, e a captura do script passa a ter de mostrar conteúdo. Só vale em app de debug: no de produção, o Android recusa e a tela segura fica.

Com `-Relay -TelaLivre`, o script também captura as telas dos dois celulares sozinho durante o teste: a cada linha nova do app no logcat (carta enviada, selada, não abriu) e a cada 15 s. As capturas e um `eventos.txt` com o roteiro do que aconteceu vão para uma pasta `kryptos-teste-<data>` em Downloads e, no fim, para um zip de mesmo nome, pronto para anexar.

Roteiro (A e B são os dois celulares):

1. Nos dois: abrir com o PIN. A Central mostra "RELAY LIGADO".
2. Frente a frente: em NOVO CONTATO, A lê o QR de B e B lê o QR de A.
3. A: tocar no canal de B, escrever, CIFRAR. A mensagem aparece fechada, com "ENVIADA · QUEIMA EM 01:00".
4. B: em até 10 s, "1 NOVA" no canal de A. Abrir o canal: 8 grupos. Segurar: o texto aparece; soltar: some. A contagem começa em 01:00 e a mensagem some quando chega a zero.
5. B responde. A recebe da mesma forma.
6. Tentar um print em qualquer tela: sai preto ou o Android avisa que o app não deixa.
7. B: abrir uma mensagem, apagar a tela antes do minuto e acender de novo. A mensagem lida sumiu.
8. A: fechar o app (botão de recentes, o terceiro da barra, e arrastar o Kryptos para cima; ou Configurações, Apps, Kryptos, Forçar parada). B manda uma mensagem. A abre de novo. Se a conversa já tinha ida e volta, a Central mostra "1 mensagem não pôde ser aberta"; se a mensagem de B ainda era a primeira de uma conversa, ela abre (ver acima).

Depois do FIM, o script mostra o logcat de cada celular. Cada carta deixa uma linha com o caminho, sem texto nem código: `carta enviada (conversa nova | conversa aberta)`, `carta selada (conversa nova | conversa aberta)`, `carta nao abriu`, `carta ja guardada`, `carta para outra pessoa`, `caixa cheia`.
