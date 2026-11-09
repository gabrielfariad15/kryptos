# Leis do Kryptos

Estas leis valem acima de qualquer decisão técnica, recomendação, regra de trabalho ou documento do repositório. Só o Gabriel muda uma lei, e a mudança entra no histórico com data e motivo.


## Como usar

1. Antes de recomendar, desenhar ou escrever código, conferir contra estas leis.
2. Solução que fere uma lei é descartada. Não vale "mitigar" nem "tomar cuidado". Se nenhum caminho cumpre todas, parar e perguntar ao Gabriel, dizendo qual lei trava e por quê.
3. Toda resposta com recomendação traz a linha "Leis conferidas:" com as leis que ela toca.
4. Documento ou decisão antiga que contradiz uma lei não é seguida: vira conflito aberto, no fim deste arquivo, até o Gabriel decidir.


## Lei 1. Para a pessoa, funciona como o WhatsApp

- Abre, escreve, manda, chega.
- A pessoa escolhe com internet ou sem internet. Sem internet, o app mostra até quantos km consegue alcançar. Esse número vem de medida, nunca de chute.
- Nenhum outro detalhe técnico aparece: qual celular conectou, nova tentativa, falha de conexão.
- Falha de conexão é resolvida em silêncio, com nova tentativa automática.
- Os dois lados mandam e recebem, não importa quem conectou.
- Sem internet, o meio (Bluetooth ou rádio) é escolhido sozinho.


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


## Lei 9. Só mensagem de texto, e nada salvo

- O Kryptos só manda texto. Sem áudio, vídeo, foto, arquivo, chamada de voz ou chamada de vídeo.
- Motivo: no WhatsApp, até a mídia de "visualização única" fica gravada na pasta do app. No Kryptos, nada do que foi lido fica no aparelho.
- O que vale para o texto vale sem exceção: depois de lido, nada salvo (Lei 3).


## Lei 10. Um app só, no Modelo Espião

- Existe um único Kryptos, igual para todos e por todos os meios (internet, Bluetooth, rádio). Não há versão separada nem visual diferente por tipo de cliente.
- Empresas e órgãos podem usar, mas no mesmo Modelo Espião: nada de registro, nada salvo, só texto.
- Contato se adiciona de dois jeitos: lendo o QR Code da pessoa ou digitando o código de contato dela.
- A pessoa dá ao contato o nome que quiser. O nome fica só no aparelho.
- O visual aprovado é o do quadro "Kryptos — o app, Modelo Espião" (https://claude.ai/artifact/PbgzsUXbgYN43wDxhd8BmE), aprovado em 09/11/2026 e mantido.


## Lei 11. Código fechado

- O código do Kryptos não é aberto.
- A confiança vem de auditoria externa publicada, não de código aberto.
- Bibliotecas de terceiros só com licença que permite código fechado, com o aviso de licença que elas exigem (Lei 6: AGPL e GPL ficam fora).


## Conflitos abertos

Decisões já tomadas que contrariam uma lei. Não seguir até o Gabriel decidir a correção.

1. **Fechar o app encerra as conversas (contraria a Lei 2).** A sessão Olm fica só em memória. Quando o app fecha, as cartas seguintes daquela conversa não abrem mais e viram "N mensagens não puderam ser abertas". Está em `arquitetura/sessao-olm.md`, `agenda-gravada.md`, `caixa-selada.md`, `malote.md` e `telas-de-conversa.md`. Aberto em 09/11/2026.
2. **Modo Direto só com o app aberto (contraria a Lei 2 sem internet).** Com o app fechado, o celular não recebe pelo Bluetooth. Está em `arquitetura/modo-direto.md`, decisão 3. Aberto em 09/11/2026.


## Histórico

- 09/11/2026: versão 1. Motivo: recomendações que fugiam do que o Kryptos é fizeram o trabalho dar voltas. Exemplos: gravar as conversas no aparelho e pôr um número de cada celular no anúncio Bluetooth.
- 09/11/2026: Leis 9 (só texto, nada salvo), 10 (Kryptos Empresa) e 11 (código fechado), decididas pelo Gabriel. Motivo da Lei 9: a mídia de visualização única do WhatsApp fica gravada na pasta do app.
- 09/11/2026: Lei 1 mudada pelo Gabriel: dentro do app há a opção com internet e sem internet, e sem internet mostra até quantos km alcança. Lei 10: o nome do contato é escolhido pela pessoa; visual do quadro "Kryptos — o app, Modelo Espião" aprovado e mantido.
- 09/11/2026: Lei 10 reescrita pelo Gabriel. Sai a linha separada "Kryptos Empresa", com painel; entra um app só, no Modelo Espião, para pessoas e empresas, com contato por QR ou código.
