# Como trabalhar comigo em código — Kryptos

Regras para as sessões técnicas do Kryptos, para o Claude e para quem mais mexer no repositório. Cada regra nasceu de um erro real e existe para errar menos e gastar menos, não para travar o trabalho.

Duas cláusulas valem acima de todas:

- **Se eu disser "regra 4.2" (ou qualquer número), pare na hora.** Diga o que foi feito fora da regra e corrija antes de seguir.
- **Se uma regra atrapalhar mais do que ajuda numa situação, diga qual e por quê, e siga.** A exceção fica registrada na conversa. Regra que vive sendo exceção sai do documento (seção 11).

Tarefa pequena não precisa de cerimônia. Uma pergunta direta recebe uma resposta direta.


## 1. O essencial

1. Arquivo inteiro antes de propor mudança, nunca trecho.
2. Rastro antes de hipótese. Sem o erro completo, a resposta é palpite e precisa dizer que é palpite, sem código de correção.
3. Provar lendo de volta. "Rodou sem erro" não é prova.
4. Medir entre uma correção e outra. O que não moveu o número sai antes da próxima tentativa.
5. Dizer o que a solução não resolve.


## 2. Começo de cada assunto

1. Um assunto por conversa. Assunto novo, conversa nova, porque cada mensagem paga por todas as anteriores.
2. Uma foto só, no começo: `ferramentas/foto.ps1 <pastas>` zipa tudo o que o assunto toca, em vez de pedir arquivo por arquivo em rodadas.
3. Antes de medir, testar ou escolher biblioteca: uma pesquisa curta (internet e repositórios) e um resumo de poucas linhas, só com o que muda a decisão. Pesquisa para exibir que pesquisou não vale.
4. Conferir a assinatura da função no código do projeto antes de chamá-la.
5. Quando a decisão voltar para o Claude ("o que recomenda?"): a recomendação tecnicamente correta, não a mais simples nem a mais conveniente.


## 3. Mudar arquivo

1. O que é menor viaja, decidido medindo:
   - Mudança grande em arquivo de até ~20 KB: o arquivo inteiro, com codificação explícita e conferência de sha.
   - Mudança pequena em arquivo grande: patch magro.
2. Patch passa pelo `ferramentas/patch.py`:
   - Simula contra o arquivo real e exige âncora única.
   - Confere a sintaxe quando sabe.
   - Só grava com `--aplicar` e depois lê de volta.
   - Para garantir que o arquivo não mudou desde a foto, informe o `sha256_antes`.
3. Nunca `$s.Replace` de bloco longo no PowerShell.
4. Uma variável por vez. Duas mudanças juntas não dizem qual delas resolveu.
5. Codificação: UTF-8 sem BOM. Exceção: `.ps1` com acento vai com BOM, senão o Windows PowerShell 5.1 corrompe os acentos.
6. Fim de linha: fixado pelo `.gitattributes`, igual em qualquer máquina. No repositório tudo fica com LF; na pasta, CRLF só em `.ps1`, `.bat` e `.cmd`. A suíte confere. Pacote do Claude já vem no fim de linha da pasta.


## 4. Comandos

1. Comando que vai se repetir vira script em `ferramentas/`, rodado por uma linha. Script novo parte do `ferramentas/modelo.ps1`.
2. Os fatos da máquina ficam em `ferramentas/ambiente.json`, fora do git. Desktop e notebook têm usuários diferentes.
3. Bloco para colar no terminal **para no primeiro erro**, nunca segue em frente. (Lição de 06/11/2026: um bloco continuou depois de um arquivo que faltava.)
4. Tudo o que pode ir junto vai num bloco só, encadeado pelo resultado: simular, aplicar, rodar a suíte.
5. Limpeza (temporário, variável de ambiente, banco de teste) fica no mesmo bloco que sujou, no `finally`.
6. Do resultado, colar só as linhas de saída (`OK`, `FALHA`, `RESULTADO`), não o comando de volta.
7. Na mensagem, o que é **para ler** fica separado do que é **para colar**, e todo arquivo aparece com o caminho completo.


## 5. Testes e números

1. Suíte vermelha interrompe tudo.
2. Número que vai para código, documento ou commit é medido ou vem marcado "estimo".
3. Ferramenta de verificação é calibrada contra casos certos e errados antes de valer. Isenção tem motivo escrito.
4. Conferir também o que **deveria existir e não existe**, e o que se aponta olhando a tela uma vez.
5. Funcionalidade visível só conta como pronta depois de vista funcionando no aparelho.


## 6. Git

1. `git add` com arquivos ou pastas explícitos. Nunca `-A` nem `.`.
2. Comando de commit só depois da suíte verde.
3. Mensagem de commit sem rodapé de atribuição: nada de `Co-Authored-By` nem link de sessão.
4. Antes de `checkout`, `stash`, `reset` ou `clean`: dizer exatamente o que se perde e oferecer `git diff` antes.


## 7. Código

1. Erro nunca é engolido calado:
   - Em Python, `except` imprime ou registra.
   - Em Rust, nada de `unwrap()` em caminho de produção. O erro sobe com contexto.
2. Pensar antes de escrever nos ângulos de falha: entrada vazia, dado corrompido, sem rede, rede que cai no meio, aparelho sem espaço, duas mensagens ao mesmo tempo, app fechado à força, relógio do aparelho errado.
3. Código que o Claude escreve vem com o teste que prova que funciona e com um teste do caso errado.


## 8. Segurança (próprias do Kryptos)

1. Não inventar criptografia. Só bibliotecas auditadas (vodozemac, OpenMLS) e as primitivas que elas oferecem.
2. Texto de mensagem, chave e código de contato nunca vão para log, para mensagem de erro nem para tela de depuração.
3. Mensagem só em memória: nada em claro em disco, cache, banco ou arquivo temporário. Toda mudança na queima vem com um teste que procura o texto no armazenamento do app depois de queimar.
4. Toda função de criptografia tem teste com vetor conhecido e teste de adulteração: a mensagem alterada precisa ser recusada.
5. A queima conta pelo relógio monotônico do aparelho, não pela hora do sistema, que o usuário pode mudar.
6. Dependência nova: conferir a licença antes. AGPL e GPL ficam fora. A dependência é registrada em `documentacao/DEPENDENCIAS.md` com versão e motivo.
7. Segredos (chave de assinatura, `.env`) nunca entram no repositório. A pasta `segredos/` está no `.gitignore`.


## 9. Comunicação

1. Respostas por partes, por tópico. Perguntas para mim vêm numeradas.
2. Texto em inglês que eu vou enviar vem com a tradução em português junto.
3. Mensagem para terceiros começa pelo contexto (o que eu estava fazendo, o que aconteceu) e só depois traz o pedido.
4. Arquivo produzido vai anexado para download, não só citado por caminho, e nunca com o nome de um arquivo entregue antes.
5. Documento Word: tabela só para dado tabular de verdade. Aviso em vermelho, não em caixa. Sem cara de feito por IA.


## 10. Abertura de conversa técnica

Para colar no começo de cada conversa:

```
Assunto: <uma frase>
Pronto quando: <critério que dá para conferir>
Foto: <zip do ferramentas/foto.ps1, anexado>
Erro (se houver): <rastro completo>
O que já tentei: <o que foi e o que aconteceu>
```

O Claude responde primeiro com o que entendeu, o levantamento curto (2.3) se couber e o primeiro passo. Ele não escreve código antes de ter a foto.


## 11. Mudar estas regras

1. Regra nova entra quando um erro se repete ou custa caro.
2. Regra que só atrapalha sai.
3. Toda mudança tem data e motivo no histórico abaixo.


## Ferramentas

| Arquivo | Para quê |
|---|---|
| `ferramentas/criar-estrutura.ps1` | Monta as pastas do repositório numa máquina nova e gera o `ambiente.json` |
| `ferramentas/foto.ps1` | Zipa o assunto para anexar na conversa, deixando de fora segredos e lixo de compilação |
| `ferramentas/patch.py` | Aplica troca de texto com simulação, âncora única, sintaxe e conferência de sha |
| `ferramentas/modelo.ps1` | Base de script novo: para no erro, limpa no final, imprime só o resultado |
| `ferramentas/diagnostico-ambiente.ps1` | Confere, sem alterar nada, se a máquina tem Rust, Flutter, SDK/NDK do Android, gerador da ponte e o celular no cabo |
| `ferramentas/instalar-ferramentas.ps1` | Instala o que o diagnóstico apontou, com versões fixas, e grava os caminhos no `ambiente.json` |
| `ferramentas/suite.ps1` | Suíte antes de commit: fmt, clippy, testes Rust, análise e testes do Flutter; com `-Aparelho`, os testes da ponte e do cofre no celular, com as medidas |
| `ferramentas/instalar-no-celular.ps1` | Compila uma vez, instala, abre o app, confere o logcat, informa tamanho e densidade da tela e salva a captura em Downloads; com mais de um celular, `-Todos` ou `-Serial` |
| `ferramentas/aplicar-pacote.ps1` | Aplica um pacote de arquivos inteiros entregue pelo Claude: confere o sha de antes e de depois e lê de volta |
| `ferramentas/conferir-fim-de-linha.ps1` | Confere pelo `git ls-files --eol` se todo arquivo rastreado segue o `.gitattributes`; a suíte chama |
| `ferramentas/oraculos/cofre.py` | Recalcula, sem o código Rust, os vetores conhecidos do cofre da identidade (só quando o formato mudar) |
| `ferramentas/normalizar-fim-de-linha.ps1` | Aplica o `.gitattributes` ao que já existe: `git add --renormalize` e troca só o fim de linha na pasta |


## Histórico

- 06/11/2026: versão 1, criada do zero para o Kryptos a partir das regras do Lastro e das lições do dia (um bloco que seguia depois do erro; um arquivo da máquina indo para o git). Ganhou as regras de segurança próprias do projeto (seção 8) e a abertura de conversa (seção 10).
- 06/11/2026: tabela de ferramentas com os cinco scripts da fase 1 (diagnóstico, instalação, suíte, instalação no celular, aplicação de pacote).
- 06/11/2026: fim de linha fixado pelo `.gitattributes` (regra 3.6), conferido pela suíte. Motivo: o `core.autocrlf` do Windows deixava o mesmo arquivo com sha diferente entre máquinas, e `.cmd` com LF pode falhar no cmd.exe.
- 07/11/2026: tabela de ferramentas com o oráculo do cofre; a suíte com `-Aparelho` passa a provar o cofre da identidade no celular.
