# Cofre da identidade — formato v1

Decidido em 07/11/2026. Código: `nucleo/src/cofre.rs` (formato e disco), `nucleo/src/aparelho.rs` (etapas), `frontend/app/android/.../ChaveDoAparelho.kt` (Keystore), `frontend/app/lib/cofre_do_aparelho.dart` (ligação).

## Para que serve

Guarda a identidade do aparelho (a conta Olm do vodozemac) cifrada no disco, para o código de contato não mudar a cada abertura. Quem abre é o PIN de 6 dígitos junto com uma chave HMAC do Keystore do Android, que não sai do hardware (TEE ou StrongBox).

## Como a chave sai do PIN

```
esticado        = Argon2id(PIN, sal, custo)                       núcleo
chave_do_cofre  = HMAC-SHA256 do Keystore sobre esticado          Kotlin
chave_da_conta  = HKDF-SHA256(sal = nonce, ikm = chave_do_cofre,
                              info = "KRYPTOS/cofre/v1/conta" || cabeçalho)   núcleo
conta cifrada   = cifra de conta do vodozemac com chave_da_conta
```

- O Keystore vem por último de propósito. Quem copiar o arquivo não consegue testar PINs fora do aparelho, porque cada tentativa precisa do HMAC feito pelo hardware.
- Não existe verificador do PIN gravado. PIN errado gera outra chave e a autenticação da conta recusa. Pela cifra não dá para distinguir PIN errado, arquivo alterado ou chave do Keystore diferente; o app mostra "PIN incorreto".
- O nonce muda a cada gravação. A cifra de conta do vodozemac tira o IV da própria chave, então repetir a chave repetiria o IV.
- O cabeçalho entra no `info` do HKDF: qualquer byte alterado nele muda a chave e a conta não abre.
- Custo do Argon2id: 19 MiB, 2 passadas, 1 linha (mínimo do OWASP). Fica gravado no cabeçalho, para poder subir depois sem perder cofres antigos. O tempo no Motorola sai na linha `MEDIDA` do teste no aparelho.
- Medido em 07/11/2026 no Motorola Edge 40 Neo (Android 15, build de debug com o Argon2 otimizado): criar 107 ms, abrir 69 ms (Argon2id + HMAC + disco), chave no TEE (sem StrongBox), varredura de 6 arquivos sem nenhuma ocorrência.

## Arquivo

Pasta: `noBackupFilesDir` do app (fora de qualquer backup). O app também tem `android:allowBackup="false"`.

`identidade.kryptos`:

| Bytes | Campo |
|---|---|
| 8 | `KRYCOFRE` |
| 1 | versão (1) |
| 16 | sal do Argon2id |
| 4 + 4 + 4 | memória em KiB, passadas, linhas (u32, little-endian) |
| 16 | nonce da gravação |
| resto | conta cifrada pelo vodozemac (base64, ASCII) |

`tentativas`: falhas seguidas, em texto. A tentativa é registrada antes da conferência, então fechar o app no meio não apaga o erro. Acerto zera.

Gravação: arquivo temporário, `fsync`, troca de nome e `fsync` da pasta. App fechado no meio não deixa cofre pela metade. O temporário só tem o que já está cifrado.

Ao ler, o custo do cabeçalho é limitado (até 1 GiB, 10 passadas, 4 linhas), para um arquivo adulterado não pedir memória demais ao aparelho.

## Espera entre tentativas

Com até 4 falhas seguidas, a próxima tentativa não espera. Depois de 5 falhas seguidas, cada tentativa espera 30 s, dobrando a cada nova falha, até 1 hora. A espera é cumprida dentro da própria tentativa, antes do Argon2id.

Contador apagado ou ilegível com cofre presente conta como 5 falhas: apagar o arquivo não zera a espera.

## Etapas

Criar (primeira abertura, depois de GERAR CÓDIGO e do PIN repetido):

1. Kotlin troca a chave do alias por uma nova (StrongBox se houver; senão TEE).
2. Núcleo: sal novo + Argon2id.
3. Kotlin: HMAC.
4. Núcleo: sela a identidade da memória e grava. Recusa se já existe cofre.

Abrir (aberturas seguintes):

1. Núcleo: registra a tentativa, cumpre a espera e faz o Argon2id com o sal do arquivo.
2. Kotlin: HMAC.
3. Núcleo: abre, zera as tentativas e põe a identidade na memória.

O PIN, o resultado do Argon2id e a chave do Keystore passam pelo Dart em `Uint8List` e são zerados depois do uso. O núcleo recebe em `Zeroizing`.

## Regravar (chaves avulsas)

Com o cofre aberto, o `aparelho` guarda na memória a chave do cofre, o sal e o custo, e regrava a conta quando ela muda: chave avulsa gerada ou gasta. A regravação tem nonce novo, mesma gravação atômica (`cofre::regravar`, que recusa se não há cofre) e releitura que compara a conta inteira. Regras e medidas: `documentacao/arquitetura/sessao-olm.md`, seção "Chaves avulsas no cofre".

## Vetores conhecidos

Calculados por `ferramentas/oraculos/cofre.py` (argon2-cffi, que usa a implementação de referência em C, e a biblioteca `cryptography`), sem o código Rust, e conferidos pelos testes `vetor_conhecido_*` de `cofre.rs`:

- Argon2id barato (64 KiB, 1 passada) e no custo padrão.
- Cabeçalho e chave da conta.
- Arquivo inteiro, com a conta fixa das chaves da RFC 8032 (teste 1) e da RFC 7748 (Alice). O código dessa conta é `2J2F-P63Y`, o mesmo do vetor do código de contato.

## Como é provado

- Núcleo: vetores conhecidos; cada bit do arquivo alterado é recusado; arquivo cortado em cada posição dá erro sem pânico; chave errada não abre; custo adulterado é recusado antes do Argon2id; contador apagado não zera a espera.
- Varredura: procura as chaves secretas (as duas da identidade e a de cada chave avulsa ainda não gasta) em 8 formas (bytes, hex maiúsculo e minúsculo, 4 variantes de base64, lista JSON). Calibrada contra os casos certos (acha cada forma plantada; acha a conta gravada em claro) e o errado (não acha nada no cofre).
- No aparelho (`integration_test/cofre_test.dart`): criar, tirar da memória e abrir devolve o mesmo código; PIN errado não abre; sem a chave do Keystore não abre; a chave está no hardware; a varredura da pasta inteira do app dá zero.

## O que não resolve

- **Celular com root:** o atacante usa o Keystore no próprio aparelho, zera o contador e testa PINs. Só o Argon2id segura: 1 milhão de PINs a 69 ms cada dá cerca de 19 horas para testar todos (estimo, a partir da abertura medida, que inclui o HMAC e o disco). Subir o custo é decisão à parte.
- **Esqueceu o PIN, perdeu a identidade.** Não há recuperação, porque não há conta em servidor.
- **Limpar os dados do app ou desinstalar** apaga a chave do Keystore, e o cofre não abre mais. A tela avisa (`CHAVE_AUSENTE`), mas ainda não há saída para recomeçar a identidade: é assunto à parte (junto do "apagar tudo").
- **Memória:** com o cofre aberto, a conta fica na memória do processo. Quem lê a memória do app (root, depurador) lê a chave. O Dart não garante que cópias antigas dos bytes sumiram antes do coletor de lixo passar. Em especial, o HMAC chega do canal do Android somente leitura: o app zera a cópia que faz, mas o original fica até o coletor passar.
- **Chave do cofre na memória:** com o cofre aberto, ela fica na memória para regravar. Quem lê a memória do app já lê a conta, então o risco a mais é pequeno.
- **A varredura não procura o PIN**, porque 6 dígitos aparecem por acaso em qualquer arquivo binário.
- **iPhone e computadores:** não têm este Keystore. Fica para quando chegar a vez deles.
- **Trocar de aparelho:** a identidade não sai do aparelho, por decisão. Aparelho novo é identidade nova.
