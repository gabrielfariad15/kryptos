# Relay na internet a partir do PC (Tailscale Funnel)

Etapa de teste: os dois celulares conversam pela internet, com o relay rodando no PC do Gabriel. Os celulares não têm Tailscale, então tanto no Wi-Fi de casa quanto no 4G o caminho é o mesmo: DNS público, servidor do Funnel, PC. Não é a implantação definitiva. Quando outras pessoas usarem o app, o relay vai para um servidor próprio (VPS), com o proxy na frente (Caddy ou nginx) que o `estoque-de-chaves.md` e o `malote.md` pedem.

## Por que o Funnel

Decisão de 08/11/2026, entre três caminhos para expor o PC:

- **Tailscale Funnel:** o TLS termina no próprio PC (o servidor do Funnel repassa os bytes sem abrir), o endereço é `https://<pc>.<tailnet>.ts.net` com certificado válido emitido pelo Tailscale, e não depende de abrir porta no roteador.
- **Cloudflare Tunnel:** o TLS termina na Cloudflare, que vê o conteúdo das requisições. As cartas já vão cifradas de ponta a ponta, mas o pedido de retirada mostra de quem é a chave pedida. Ficou de fora.
- **Porta aberta no roteador:** esbarra no CGNAT, comum nas operadoras do Brasil (não medido na operadora do Gabriel), e expõe o IP de casa.

Fatos do Funnel que mudam o que o script faz (documentação do Tailscale, conferida em 08/11/2026):

- Precisa de MagicDNS, HTTPS ligado e do atributo `funnel` na política da tailnet. Na primeira vez, o `tailscale funnel` mostra um link para aprovar, e a aprovação liga o HTTPS e o atributo.
- Quando o Tailscale não espera a aprovação, o comando sai com código 0 sem ligar nada (código-fonte do CLI, `enableFeatureInteractive`). Por isso o script lê a configuração de volta (`tailscale funnel status --json`) em vez de confiar no código de saída.
- Só nas portas 443, 8443 e 10000, só nomes `*.ts.net`, só por TLS. O repasse para serviço local aceita só `http://127.0.0.1`, e o relay já escuta só ali.
- Com `--bg`, a configuração fica até ser desligada e volta sozinha depois de reiniciar o PC. O script desliga no FIM.
- O DNS público do nome pode levar até 10 minutos na primeira vez.
- O tráfego tem limite de banda que não dá para configurar. O valor não é publicado.
- A sintaxe do comando mudou na versão 1.52; o script exige a 1.52 ou mais nova.

## Como roda

Duas janelas do PowerShell.

**Janela 1, o relay (fica aberta durante o teste):**

```
powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\relay-publico.ps1
```

1. Confere o Tailscale: instalado, 1.52 ou mais nova, conectado, nome `*.ts.net`.
2. Confere que a porta 443 do Funnel não serve outra coisa. Se servir, para sem mexer nela.
3. Compila o relay (release) e sobe em `127.0.0.1:18090`, com o malote em arquivo. A 18090 é diferente da 18080 do relay de teste da suíte, para os dois poderem rodar juntos.
4. Liga o Funnel na 443 apontando para o relay e lê a configuração de volta: tem de apontar para `http://127.0.0.1:18090` e estar aberta para a internet.
5. Confere o `/saude` por dentro (pela tailnet, o que prova o certificado e o repasse) e por fora: pergunta o nome ao DNS público por HTTPS (DNS sobre HTTPS do 1.1.1.1) e vai ao servidor do Funnel com `curl --resolve`, o mesmo caminho do celular sem Tailscale. IP da tailnet (100.64.0.0/10) nunca conta como caminho público. O teste por fora espera até 10 minutos (`-MinutosPorFora`). Se não responder, fica como AVISO, com o endereço para conferir no navegador do celular, e o relay continua no ar.

   Por que por HTTPS: com o Tailscale ligado, o Windows desvia as consultas de `*.ts.net` para o DNS do Tailscale, mesmo pedindo ao 1.1.1.1. Na primeira versão (`Resolve-DnsName -Server 1.1.1.1`), o teste de 08/11/2026 deu "por fora OK via 100.89.224.80", que é o próprio PC na tailnet: não provava o caminho público.
6. Grava o endereço em `%LOCALAPPDATA%\Kryptos\relay-publico\endereco.txt` e lê de volta.
7. Espera FIM. No fim: apaga o arquivo do endereço, desliga o Funnel (e confere que a 443 ficou vazia), desliga o relay e mostra as últimas linhas do log dele. O relay não grava chave, dono nem conteúdo no log (regra 8.2).

**Janela 2, os celulares (no cabo só para instalar, ler o logcat e capturar):**

```
powershell -ExecutionPolicy Bypass -File C:\kryptos\ferramentas\instalar-no-celular.ps1 -Todos -RelayPublico -TelaLivre
```

O `-RelayPublico` lê o endereço do arquivo, confere o `/saude` a partir do PC (pela tailnet: não prova o caminho público) e monta o app com esse endereço. Não faz `adb reverse`: o celular vai ao relay pela internet, e o cabo não entra no caminho. Diz se o Wi-Fi de cada celular está ligado, só como informação. Espera FIM e captura as telas como o `-Relay`. Não combina com `-Relay`.

O endereço fica gravado no app. Se o relay não estiver no ar, o app mostra falha de rede e tenta de novo a cada 10 s.

## Onde ficam os dados

- **Malote:** `%LOCALAPPDATA%\Kryptos\relay-publico\malote.redb`, fora do repositório. Guarda as cartas ainda não buscadas (validade de 7 dias) e sobrevive entre uma vez e outra. Para zerar, apague o arquivo com o relay desligado.
- **Estoque de chaves avulsas:** só na memória. Quando o relay volta, o app repõe as chaves na próxima busca, porque publica sempre que o estoque fica abaixo de 20 (`correio.dart`).
- **Logs do relay:** na mesma pasta durante a execução. São mostrados e apagados no FIM.

## Teste pela internet

1. Janela 1: `relay-publico.ps1` até aparecer `RELAY NA INTERNET`. Não digitar nada nela até o fim do teste.
2. Nos celulares: Wi-Fi ligado (ou dados móveis que funcionem) e Não perturbe ligado, para notificação de outro app não entrar nas capturas automáticas.
3. No navegador do celular: `https://<endereço>/saude`. Tem de aparecer `ok 0.1.0`. É a prova do caminho público; as conferências feitas do PC passam pela tailnet.
4. Janela 2: `instalar-no-celular.ps1 -Todos -RelayPublico -TelaLivre`, até `Digite FIM para encerrar`. Não digitar FIM ainda.
5. O roteiro de `telas-de-conversa.md` ("Teste com os dois celulares"), passos 1 a 8.
6. Só depois: FIM na janela 2, depois FIM na janela 1.

Não desligar o Wi-Fi para "forçar o 4G" sem antes abrir um site com os dados móveis: no teste de 08/11/2026 os celulares ficaram sem internet nenhuma e o app mostrou "sem conexão com o relay".

## Resultado de 08/11/2026

- Relay em `https://relay.tail9633d5.ts.net`, Tailscale 1.104.5 no Windows 11.
- Navegador do celular no Wi-Fi de casa: `ok 0.1.0`.
- Os dois celulares (motorola edge 40 neo e moto g15, Android 15) no Wi-Fi de casa: Central com "RELAY LIGADO", mensagem do edge para o g15 e resposta do g15 para o edge, cifradas, abertas segurando. Visto nos celulares; a sessão não teve logcat nem capturas guardadas.
- Não testado: 4G (os dados móveis do celular testado não funcionaram), celulares longe de casa.

## O que esta etapa não resolve

- **Sem limite por endereço de rede.** O Funnel não tem, e o relay de propósito não lê IP. Quem tiver o endereço pode encher o malote de um destinatário até o limite (500 cartas ou 8 MiB) e o relay até os limites totais (2 GiB, 20.000 destinatários). A retirada de chaves avulsas tem o limite por dono do estoque (10 seguidas, depois 1 a cada 6 min). Serve para o teste com dois celulares; não serve para abrir a outras pessoas.
- **O nome do PC fica público.** O certificado do `*.ts.net` entra nos registros públicos de certificados (Certificate Transparency), então o nome pode ser achado e varrido. Antes da primeira vez, vale trocar o nome da máquina no painel do Tailscale para algo neutro, como `relay`.
- **O Tailscale vê os metadados.** O servidor do Funnel não abre o TLS, mas vê o IP de quem conecta, a hora e o tamanho do tráfego.
- **Depende do PC.** Ligado, acordado e com o script aberto. Se o PC dormir, o relay some até ele acordar.
- **Funnel é recurso do Tailscale, com limite de banda não publicado.** Pode mudar sem aviso.
- **Fechar a janela pelo X deixa o Funnel ligado** apontando para um relay desligado. Rodar o script de novo reaproveita a configuração, ou: `tailscale funnel --https=443 off`.

## Calibração dos scripts (08/11/2026, no Linux, com `tailscale` e `curl` falsos)

O `relay-publico.ps1` foi rodado contra um `tailscale` falso, que imita a saída do `status --json` e do `funnel status --json` conferida no código-fonte do Tailscale (`ipn/serve.go`, `ipn/ipnstate`), e um `curl` falso, que imita o repasse do `*.ts.net` para o relay.

- **Casos certos:** caminho feliz (RESULTADO OK, Funnel lido de volta e desligado, relay desligado, arquivo do endereço apagado). Sobra de uma vez anterior com a barra final no Proxy: reaproveitada.
- **Casos errados, cada um com a FALHA esperada e nada sobrando:** Tailscale desconectado, versão 1.50, nome fora de `*.ts.net`, Funnel que sai com 0 sem ligar, Funnel só dentro da tailnet, `tailscale funnel` com erro, porta 443 servindo outra coisa (deixada intacta), repasse quebrado, porta 18090 ocupada, desligar o Funnel falhando (FALHA que manda desligar à mão) e relay morrendo durante o teste.
- **DNS sobre HTTPS (segunda versão):** IP público devolvido: OK via esse IP. Só IP da tailnet: AVISO, nunca OK. Nome ausente, resposta que não é JSON e falha da consulta: AVISO com o motivo.
- **Defeitos que a calibração achou e foram corrigidos antes da entrega:**
  - a vírgula valendo antes do `+` dentro de `@(...)`, que quebrava o argumento do curl;
  - uma função chamada `Curl`, que chamava a si mesma em vez do curl;
  - o indexador `Properties["/"]`, que não acha a chave `/`;
  - a saída do `tailscale funnel` virando o detalhe do passo, o que escondia o link de aprovação;
  - a variável `$nome` sendo lida como o parâmetro `$Nome` do `Passo`, por escopo dinâmico.

O `instalar-no-celular.ps1 -RelayPublico` foi conferido numa cópia isolada, com `adb` e `flutter` falsos. Sem o arquivo do endereço, com o endereço sem https e com o relay desligado: FALHA com o motivo. Com o relay no ar: o `flutter build` recebeu `--dart-define=KRYPTOS_RELAY=https://...ts.net`. Combinado com `-Relay`: FALHA.

O que não dá para provar aqui: o Tailscale de verdade, o DNS público e o celular no 4G. Isso é o teste no PC e nos celulares.

O que a calibração deixou passar e o PC mostrou: o desvio de `*.ts.net` para o DNS do Tailscale no Windows (no Linux da calibração não havia Tailscale). Corrigido com a consulta por HTTPS e a recusa do IP da tailnet.
