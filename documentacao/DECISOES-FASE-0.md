# Kryptos — decisões até 06/11/2026

Resumo do que foi decidido antes de abrir o repositório. Serve de ponto de partida para o projeto da fase 1. Destino sugerido no repositório: `documentacao/DECISOES-FASE-0.md`.

Repositório: https://github.com/gabrielfariad15/kryptos
Telas aprovadas: https://claude.ai/artifact/2UhWbFxWehAmzf8KX2qnxt (linha "KRYPTOS unificado" e "telas de apoio")


## 1. O que é

Mensageiro privado com criptografia de ponta a ponta e mensagens que somem depois de lidas. Uso próprio e de quem o Gabriel disponibilizar; venda depois.

- Contato por código ou QR, sem número de celular, e-mail ou cadastro.
- Mensagem para quem está offline fica guardada cifrada no servidor e é apagada na entrega. Validade padrão: 7 dias.
- O tempo de queima começa na leitura, não no envio. Padrão: 1 minuto.
- A mensagem fica só na memória do aparelho e nunca é gravada em disco.
  - Revisto em 07/11/2026: a mensagem recebida e ainda não lida fica gravada **selada** no aparelho, numa gaveta de tamanho fixo cuja chave mora no Keystore, até a pessoa segurar para ler. Depois da leitura, conta 1 minuto e a gaveta é sobrescrita e a chave trocada. Em claro, o texto continua só na memória. Ver `documentacao/arquitetura/caixa-selada.md`.
- Conversa entre duas pessoas e em grupo.
- Alvo final: Android, iPhone, Windows, macOS e Linux.


## 2. Nome

- Nome escolhido: KRYPTOS.
- Pendência: buscar no INPI (classes 9, 38 e 42) e na Play Store. É palavra grega comum e também o nome da escultura cifrada da sede da CIA. O visual não usa nome, brasão nem marcação oficial de nenhum órgão real.
- Codinomes de módulo propostos, ainda não fechados: SONAR (proximidade), CINZA (apagamento), TARJA (texto coberto e marca d'água), LACRE (atestação), MALOTE (fila offline).


## 3. Os quatro modos (aprovados)

| Modo | Tela | Finalidade |
|---|---|---|
| 01 | Central | Canais, conferência de chave, contatos |
| 02 | Conversa | Duas pessoas; mensagem em grupos cifrados, segurar para decifrar |
| 03 | Grupo | Um para muitos e fila para quem está offline (fita no rolo) |
| 04 | Direto | Sem internet: perto (Bluetooth, Wi-Fi Direct, som), local (mensagem presa a um lugar) e rádio de longa distância |

Telas de apoio aprovadas: novo contato (QR, código, som), criar grupo, configurações. Falta desenhar a primeira abertura (gerar código e criar PIN).


## 4. Visual (aprovado)

- Fundo preto quente `#0D0B07`, painel `#17130B`, linha `#3A2E14`.
- Texto branco-osso `#EDE6D6`, texto secundário `#A08D66`.
- Âmbar `#FFB000` para estrutura: botões, cabeçalhos, seleção.
- Laranja `#FF6A13` só para alerta e destruição: contagem de queima, chave não conferida, apagar tudo.
- Fonte única: IBM Plex Mono.


## 5. Proteções combinadas

- Bloqueio de print e gravação: `FLAG_SECURE` no Android. Em celular com root dá para burlar.
- Segurar para ler e soltar para esconder.
- Marca d'água discreta com o código de quem está lendo.
- Mensagem mostrada em partes, nunca a conversa inteira.
- Conferência de chave pessoalmente (12 grupos de 4 caracteres) contra alguém no meio.
- Grupo só aceita membro com chave conferida.
- Apagar tudo de emergência. Para funcionar de verdade num SSD, os dados ficam cifrados e o apagamento destrói a chave.


## 6. Atestação do aparelho

- Decisão: atestação por hardware do Android (Key Attestation), verificada pelo nosso próprio servidor.
- Nenhum serviço do Google é chamado e não há Play Integrity nem Google Play. Nenhum dado do usuário vai para o Google.
- A única peça externa é o certificado raiz gravado de fábrica no chip, que é do Google. Ele fica guardado como arquivo de chave pública no nosso servidor.
- O servidor só entrega mensagem para app original em aparelho com sistema original. Aparelho não atestado aparece para quem envia, e quem envia decide.
- No modo offline, o servidor emite um selo de aparelho verificado com validade, e os aparelhos conferem o selo entre si.
- Computadores e iPhone não têm equivalente sem Apple ou Microsoft. Decidir quando chegar a vez deles.


## 7. Base técnica

- Núcleo em Rust: criptografia, protocolo, queima e memória. Tela em Flutter, ligada pelo `flutter_rust_bridge`.
- Conversa entre duas pessoas: `vodozemac` (Olm/Double Ratchet), licença Apache-2.0, com uma auditoria da Least Authority sem achados graves.
- Grupo: `OpenMLS` (MLS, RFC 9420), licença MIT.
- **Não usar `libsignal`:** licença AGPL-3.0, que obriga a abrir o código do app inteiro.
- **Não usar a ponte pronta da vodozemac para Dart no pub.dev:** licença AGPL-3.0. Fazer a ponte própria a partir do crate em Rust.
- Servidor de repasse: guarda só mensagem cifrada, com validade, e apaga ao entregar.
- Peso: estimo de 10 a 25 MB no celular.


## 8. Conexões

- **Internet:** opção de roteamento em camadas (3 servidores, nenhum sabe origem e destino ao mesmo tempo) para esconder o IP. Mais lento.
- **Perto:** Bluetooth e Wi-Fi Direct. Entre Android e iPhone o app precisa estar aberto e é lento.
- **Som (ultrassom):** só para trocar o código de contato. Estimo dezenas de bytes por segundo.
- **Local:** mensagem só abre num raio definido (por exemplo, 30 m). A conferência é feita no aparelho, e a coordenada não vai para o servidor.
- **Rádio de longa distância:** rádio LoRa pequeno ligado ao celular por Bluetooth, no modelo do projeto aberto Meshtastic. Exige hardware extra por pessoa.
- **Cubo:** estima distância juntando Bluetooth, Wi-Fi, GPS e som (Kalman). A elipse no mapa mostra a incerteza.


## 9. O que a solução NÃO resolve

- Quem lê pode decorar, anotar ou fotografar com outro celular. A marca d'água só dissuade.
- Um app modificado pode não apagar. A atestação dificulta, mas computadores ficam sem esse controle.
- Metadados: o servidor vê IP e horário, a menos que o roteamento em camadas esteja ligado.
- O GPS pode ser falsificado em celular com root. O Cubo dificulta, mas não elimina.
- iPhone não tem bloqueio oficial de print.


## 10. Pendências antes de vender

- Busca de marca no INPI (classes 9, 38 e 42).
- Avaliação jurídica: com a Torion operando servidores como empresa, o Marco Civil (art. 15) exige guardar registros de acesso. Ver com advogado como isso se encaixa no modelo.
- Abrir o CNPJ da Torion Systems.


## 11. Ordem de construção

1. **Fase 1:** Android, conversa entre duas pessoas, contato por código e QR, mensagem só em memória, queima 1 minuto após a leitura, bloqueio de print, segurar para ler.
2. Marca d'água e mensagem em partes.
3. Atestação por hardware.
4. Grupos (MLS).
5. iPhone e computadores.
6. Roteamento em camadas.
7. Modo Direto (Bluetooth e Wi-Fi), som, local e rádio.
