# Leis do Kryptos

Estas leis valem acima de qualquer decisão técnica, recomendação, regra de trabalho ou documento do repositório. Só o Gabriel muda uma lei, e a mudança entra no histórico com data e motivo.


## Como usar

1. Antes de recomendar, desenhar ou escrever código, conferir contra estas leis.
2. Solução que fere uma lei é descartada. Não vale "mitigar" nem "tomar cuidado". Se nenhum caminho cumpre todas, parar e perguntar ao Gabriel, dizendo qual lei trava e por quê.
3. Toda resposta com recomendação traz a linha "Leis conferidas:" com as leis que ela toca.
4. Documento ou decisão antiga que contradiz uma lei não é seguida: vira conflito aberto, no fim deste arquivo, até o Gabriel decidir.


## Lei 1. Para a pessoa, funciona como o WhatsApp

- Abre, escreve, manda, chega.
- Nenhum detalhe técnico aparece: qual celular conectou, qual meio, nova tentativa, falha de conexão.
- Falha de conexão é resolvida em silêncio, com nova tentativa automática.
- Os dois lados mandam e recebem, não importa quem conectou.
- O meio (internet, Bluetooth, rádio) é escolhido sozinho.


## Lei 2. Com o app fechado, a mensagem chega; ao abrir, ela está lá

- Mensagem recebida com o app fechado fica guardada, cifrada no relay ou selada no aparelho, e aparece quando a pessoa abre o app.
- Vale pela internet e sem internet.
- Nenhuma mensagem se perde porque o app de quem recebe fechou.


## Lei 3. Depois de lida, é apagada

- A mensagem só é decifrada quando a pessoa segura para ler.
- Queima 1 minuto depois de lida, contado pelo relógio monotônico do aparelho.
- Ao sair do app ou apagar a tela, o que já foi lido queima na hora.
- Em claro, só na memória. Nada em claro em disco, cache, log ou tela de depuração.


## Lei 4. Sem rastro

- Sem número de celular, e-mail ou cadastro. Contato por código ou QR.
- Localização nunca é pedida por padrão e nunca em segundo plano. Só no modo Local, se a pessoa ligar, e a distância vem só do GPS, nunca do Bluetooth, do Wi-Fi ou do som.
- Nada transmitido no ar identifica o aparelho ou a pessoa. O anúncio Bluetooth é igual em todo celular com Kryptos. Quem é quem só se sabe dentro da conexão cifrada.
- Endereço Bluetooth nunca é gravado.
- Sem confirmação de leitura e sem aviso de "digitando".
- O relay não sabe quem mandou.
- Texto de mensagem, chave e código de contato nunca vão para log.


## Lei 5. Sem Google e sem Apple

- Nenhum serviço do Google: nem Play Services, Nearby, Play Integrity ou Firebase.
- Atestação por hardware, conferida pelo nosso próprio servidor.
- Nenhum dado do usuário vai para terceiros.


## Lei 6. Criptografia só auditada

- vodozemac para conversa entre duas pessoas e OpenMLS para grupo.
- Nada de cifra inventada. Morse, som ou luz só servem como meio de transmissão.
- Biblioteca com licença AGPL ou GPL fica fora.


## Lei 7. Funciona com e sem internet

- Internet pelo relay. Perto pelo Bluetooth. Longe pelo rádio LoRa. Mensagem presa a um local, se a pessoa quiser.
- Mensagem para quem não está ao alcance espera cifrada e sai sozinha quando o contato aparecer.


## Lei 8. Proteções na leitura

- Bloqueio de print, segurar para ler, marca d'água com o código de quem lê.
- Conferência de chave pessoalmente.
- Apagar tudo de emergência.


## Conflitos abertos

Decisões já tomadas que contrariam uma lei. Não seguir até o Gabriel decidir a correção.

1. **Fechar o app encerra as conversas (contraria a Lei 2).** A sessão Olm fica só em memória. Quando o app fecha, as cartas seguintes daquela conversa não abrem mais e viram "N mensagens não puderam ser abertas". Está em `arquitetura/sessao-olm.md`, `agenda-gravada.md`, `caixa-selada.md`, `malote.md` e `telas-de-conversa.md`. Aberto em 09/11/2026.
2. **Modo Direto só com o app aberto (contraria a Lei 2 sem internet).** Com o app fechado, o celular não recebe pelo Bluetooth. Está em `arquitetura/modo-direto.md`, decisão 3. Aberto em 09/11/2026.


## Histórico

- 09/11/2026: versão 1. Motivo: recomendações que fugiam do que o Kryptos é fizeram o trabalho dar voltas. Exemplos: gravar as conversas no aparelho e pôr um número de cada celular no anúncio Bluetooth.
