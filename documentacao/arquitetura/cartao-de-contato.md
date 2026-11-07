# Cartão de contato — formato v1 (o texto do QR)

Decidido em 07/11/2026. Código: `nucleo/src/cartao.rs`.

## Para que serve

É o texto que vai dentro do QR. Diferente do código de contato, que tem só 35 bits, o cartão leva as duas chaves públicas inteiras. Por isso ler o QR pessoalmente confere a chave: o aparelho que lê recebe a chave verdadeira de quem está na frente dele.

Ler o QR confere a chave num sentido só. Para os dois lados ficarem conferidos, cada aparelho lê o QR do outro.

## Formato

```
KRYPTOS1-<chaves>-<código>
  chaves = ed25519 (32 bytes) || curve25519 (32 bytes), em base32 Crockford maiúsculo:
           512 bits + 3 bits zero = 103 caracteres
  código = o código de contato dessas chaves, sem hífen (8 caracteres)
```

- São 121 caracteres, todos do modo alfanumérico do QR, o mais compacto para texto.
- O código no fim confere o texto inteiro: 35 bits do SHA-256 mais o verificador Luhn. É também o código que a outra pessoa vê na tela, então dá para comparar a olho depois da leitura.
- A leitura é estrita: sem minúscula, sem O por 0, sem espaço. O texto vem do QR, não de digitação, e só existe uma grafia por cartão.
- Os 3 bits de enchimento do último caractere das chaves têm de ser zero.
- Mudar o formato exige trocar o `1` do prefixo.

## Recusas

A ordem da conferência define o aviso que a pessoa vê:

| Código | Quando |
|---|---|
| `QR_NAO_E_KRYPTOS` | Não começa por KRYPTOS (site, Wi-Fi, outro app) |
| `QR_VERSAO_DESCONHECIDA` | Começa por KRYPTOS mas não é KRYPTOS1- |
| `QR_TAMANHO` | Diferente de 121 caracteres |
| `QR_FORMATO` | Separador fora do lugar, caractere fora do alfabeto, enchimento diferente de zero |
| `QR_NAO_CONFERE` | O código do fim não é o das chaves: QR danificado ou adulterado |
| `QR_CHAVE_INVALIDA` | A chave Ed25519 não é ponto da curva, com o código certo |

O código é conferido antes da chave. Assim, um QR danificado diz "não confere", e não "chave inválida".

## Vetor conhecido

Chaves Ed25519 do teste 1 da RFC 8032 e X25519 da Alice da RFC 7748, as mesmas do vetor do código de contato:

```
KRYPTOS1-TXD9G0C2P45BFNABZV9WJS07787E2WQKVAK269DF08D6HXR7A4D8A87G164K19TMEJ5QVQ5M7VVNM3DZ786JCE0TYKNT9ACENADMWTG-2J2FP63Y
```

Calculado por `ferramentas/oraculos/cartao.py`, só com a biblioteca padrão do Python (base32 e Luhn escritos à parte) e calibrado pelo código `2J2F-P63Y`. O teste `vetor_conhecido` confere o resultado.

## Como é provado

- Ida e volta com 200 identidades novas.
- Adulteração: cada caractere depois do prefixo, trocado por cada um dos outros 31 símbolos, é recusado. São 3.441 casos: 3.413 por `QR_NAO_CONFERE` e 28 por `QR_FORMATO`, todos no último caractere das chaves, onde a troca mexe no enchimento.
- Calibração dos testes (07/11/2026): quatro sabotagens no código foram pegas, uma por vez. Sem a conferência do enchimento, falharam 2 testes; sem a conferência do código, 2; aceitando chave Ed25519 inválida, 1; com o `Debug` mostrando o texto, 1.

## Agenda (nucleo/src/contatos.rs)

O cartão lido entra na agenda desta execução do app, conferido por QR. A agenda fica só em memória: some quando o app fecha e também quando o cofre é aberto de novo. Guardar em disco, cifrado, é assunto próprio.

| Código | Quando |
|---|---|
| `QR_PROPRIO` | O QR é o deste aparelho |
| `QR_CONFLITO` | Chaves diferentes de alguém da agenda, ou deste aparelho, mas com o mesmo código de contato ou com uma das duas chaves igual |
| `SEM_IDENTIDADE` | Não há identidade na memória para comparar |

Ler de novo o QR de quem já está na agenda não duplica o contato: a resposta é "já estava".

Mesmo código com outra chave nunca é tratado como o mesmo contato. O código tem 35 bits, e gerar uma chave com o código de outra pessoa custa horas de computador (estimo, sem medir). A agenda compara as chaves inteiras e avisa como possível imitação.

Calibração dos testes (07/11/2026): seis sabotagens, uma por vez, e cada uma derrubou pelo menos um teste. Foram tirados: a recusa do próprio QR, o conflito por código, o conflito com o próprio aparelho, o "já estava", a limpeza da agenda ao esquecer e a limpeza ao abrir o cofre. Os testes que mexem no estado global rodam em série; 20 rodadas seguidas deram 20 verdes.

## QR (nucleo/src/qr.rs)

- **Gerar:** crate `qrcode`, correção de erro M. O cartão de 121 caracteres sai quase sempre na versão 5, 37 × 37 módulos. Medido em 07/11/2026, com 3.000 cartões sorteados: 2.970 na versão 5 e 30 na versão 6 (41 × 41). O lado depende das chaves, então nenhum teste pode supor que dois cartões têm o mesmo tamanho. A tela recebe só a grade e desenha 4 módulos de margem clara em volta.
- **Ler:** crate `rxing`, só QR. Recebe o plano de brilho da câmera (1 byte por pixel) e o passo da linha, que a câmera pode completar além da largura. O texto achado ainda passa pela leitura estrita do cartão.
- **Quadro vindo de fora:** o tamanho é conferido antes de qualquer cópia (mínimo 21 × 21, máximo 16 MP, buffer do tamanho que largura, altura e passo dizem). Um pânico dentro do leitor vira o erro `QR_LEITOR` em vez de derrubar o app.
- **Na agenda:** `adicionar_contato_do_quadro` responde nada quando o quadro não tem QR (a câmera segue para o próximo) e erro quando acha um QR recusado: de outro app, adulterado ou o próprio.

Medido em 07/11/2026, no computador:

- **Leitura em fotos simuladas com desfoque** (3, 4 e 6 pixels por módulo, giro até 45°, ruído e contraste baixo): 106 de 144. O teste falha se cair abaixo de 100.
- **Tempo de um quadro de 1280 × 720:** 3 ms com ou sem QR, com o `rxing` otimizado também no build de debug. Sem essa otimização, 31 ms. O número que vale é o do celular, a medir.
- **`TryHarder` do leitor:** tirado. Não mudou nem a leitura (106 com e sem) nem o tempo.
- **Calibração:** seis sabotagens. Cinco derrubaram um teste cada: ignorar o passo da linha, tirar o lado mínimo, tirar o limite de 16 MP, alterar o texto lido e não pôr na agenda o QR achado. A sexta, tirar o `TryHarder`, não derrubou nada, e por isso ele saiu. Isenção: a troca do pânico por erro não tem teste, porque não há quadro conhecido que faça o `rxing` entrar em pânico; ela fica pelo motivo escrito acima. Os 60 quadros de lixo do teste não derrubaram o leitor nem viraram cartão.

## Ponte, câmera e tela (frontend/app)

Decidido em 07/11/2026.

- **Ponte** (`frontend/app/rust/src/api/nucleo.rs`): `grade_propria`, `adicionar_contato_do_quadro` e `contatos`. Só passam a grade do QR próprio, o código de contato e a forma de conferência. O texto do cartão e as chaves ficam no núcleo, e o erro chega como o código fixo da tabela acima.
- **Câmera** (`lib/camera_do_aparelho.dart`): plugin `camera` 0.12.2, que no Android usa o CameraX. Só a câmera traseira, em YUV 420 e resolução média (480p), sem áudio. O leitor recebe só o plano Y, com o `bytesPerRow` do próprio plano. O NV21 não foi usado: com ele o plugin continua dizendo que o formato é yuv420, e aí o formato declarado deixa de servir de prova.
- **Um quadro por vez** (`lib/leitor_de_qr.dart`): enquanto um quadro está no núcleo, os que chegam são descartados, não enfileirados. Depois de achar um contato, para.
- **Permissões:** só `CAMERA`. O plugin declara também microfone e escrita no armazenamento, e o manifesto do app tira os dois. Tira também a leitura do armazenamento, que o Android junta sozinho à escrita: a primeira instalação no Edge, em 07/11/2026, saiu com ela, e o `instalar-no-celular.ps1` recusou. Ele confere no app instalado a cada instalação.
- **Ciclo de vida:** a câmera desliga com CANCELAR, com voltar e quando o app sai da tela (`hidden` ou `paused`). Não desliga no `inactive`, que chega também com o pedido de permissão aberto por cima.
- **Tela** (`lib/telas/novo_contato.dart`): aba QR da tela aprovada. Mostra o QR próprio, o código e LER QR DO CONTATO. Depois da leitura, mostra o código lido com "CHAVE CONFERIDA · LIDA POR QR" e o botão MOSTRAR MEU QR, para o outro lado ler também. O módulo do QR tem um número inteiro de pixels da tela e é o maior que cabe na largura. As abas CÓDIGO e SOM não aparecem: ainda não há como trocar o contato por elas.
- **Medida:** cada tentativa escreve uma linha `MEDIDA qr` no logcat, com o tempo desde a câmera ligada, os quadros lidos e descartados e o tempo médio no núcleo. Nada do QR vai para essa linha. O `ferramentas/medir-leitura-qr.ps1` conta as linhas e aplica a regra de 9 em 10 em menos de 2 s.

Como é provado:

- **Ponte:** um teste em Rust, `grade_quadro_e_agenda_pela_ponte`. A identidade A mostra o QR e a B lê, num quadro com 64 bytes de sobra por linha. Também confere "já estava", o próprio QR, o quadro curto, o passo ignorado e a agenda esvaziada ao esquecer. Calibração: três sabotagens, cada uma derrubou o teste (`novo` sempre verdadeiro, módulos invertidos, margem zero).
- **No aparelho:** `integration_test/qr_test.dart` faz o mesmo caminho pelo Dart, com o núcleo de verdade, e imprime o tempo de um quadro 640 × 480 com e sem QR. Rodado também no computador contra a biblioteca da ponte para Linux (modo de distribuição): 6 de 6 verdes, 1,3 ms por quadro com ou sem QR, ponte incluída. O número que vale é o do celular.
- **Tela, no computador** (agenda e câmera falsas): 18 testes em `test/novo_contato_test.dart` e 7 em `test/leitor_de_qr_test.dart`. Calibração: oito sabotagens, cada uma derrubou um teste. Foram feitas: não fechar a câmera ao achar; não fechar ao sair da tela; fechar também no `inactive`; ignorar a sessão depois de a câmera abrir; não descartar quadros; módulo com pixel quebrado; código do contato no log; recusa sem aviso.
- **Achado no caminho:** o rodapé estourava 15 px com fonte de 160% num celular de 360 dp. O teste novo pegou, e o rodapé agora quebra a linha.

## O que não resolve

- **Chave Curve25519 de ordem baixa não é recusada na leitura.** Quem recusa é o vodozemac, ao abrir a sessão (`SessionCreationError::NonContributoryKey`, conferido no código da 0.11.2). Fica para o assunto da conversa provar com teste.
- **Quem mostra o QR pode mostrar a chave de outra pessoa.** Ela não ganha nada com isso, porque não tem a chave secreta, mas o contato fica com o nome errado. A leitura pessoal prova "esta é a chave que este aparelho mostra", não "este aparelho é desta pessoa".
- **Aparelho comprometido** mostra a chave que o atacante quiser.
- **Agenda em memória:** fechar o app perde os contatos, e eles precisam ser lidos de novo.
- **Fotos simuladas não são câmera.** Reflexo na tela, foco, tremor e tela com brilho baixo só aparecem no aparelho. Lá vale a regra de 9 em 10 leituras em menos de 2 s, medida pelo `ferramentas/medir-leitura-qr.ps1`.
- **Brilho da tela:** o app não aumenta o brilho enquanto mostra o QR. Com o brilho no mínimo, a leitura pode piorar.
- **Prévia deitada:** a prévia da câmera supõe o celular em pé. O app não trava a orientação.
- **Ida e volta:** cada lado lê o QR do outro, um depois do outro. O app não lembra o outro de ler o seu, além do texto na tela.
- **Um QR por quadro:** se houver dois QR no quadro, o leitor devolve um só, e pode ser o do outro app.
- **Print da tela:** o QR é a chave pública, não é segredo, mas quem fotografar a tela ganha o contato. O `FLAG_SECURE` ainda não está no app (a captura do G15 de 07/11/2026 saiu normal).
