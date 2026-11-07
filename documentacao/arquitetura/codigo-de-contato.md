# Código de contato — derivação v1

Decidido em 07/11/2026. Código: `nucleo/src/codigo.rs`.

## Para que serve

É o endereço curto que a pessoa digita, fala ou mostra para ser encontrada. Sai da chave pública da identidade do aparelho, sem número, e-mail nem cadastro.

O código é endereço, não prova de identidade. Quem prova que a chave é da pessoa é a conferência pessoal (12 grupos de 4) ou o QR, que leva a chave inteira. Contato adicionado só pelo código aparece como chave não conferida, em laranja.

## Como sai da chave

```
h       = SHA-256("KRYPTOS/codigo-de-contato/v1" || ed25519 || curve25519)
dados   = primeiros 35 bits de h, em 7 caracteres base32 Crockford
8º      = dígito verificador Luhn mod 32 dos 7 caracteres
exibido = XXXX-XXXX
```

- `ed25519` e `curve25519` são as duas chaves públicas de identidade da conta Olm (vodozemac), 32 bytes cada, nessa ordem.
- Alfabeto Crockford: `0123456789ABCDEFGHJKMNPQRSTVWXYZ`. Na leitura, O vira 0, I e L viram 1, minúscula vale, hífen e espaço são ignorados.
- Mudar qualquer parte da derivação exige trocar o `v1`, e todos os códigos mudam.

## Vetor conhecido

Chave Ed25519 do teste 1 da RFC 8032 e chave X25519 da Alice da RFC 7748:

- SHA-256: `1484fb187254ba7a2de11e6ae5e99248ee62089ca1650b02e3d8ffe727dc6e11`
- Código: `2J2F-P63Y`

Calculado por um oráculo em Python separado do código Rust e conferido pelo teste `vetor_conhecido`.

## O que o verificador pega

Medido em 07/11/2026 com 3.000 códigos aleatórios:

- Troca de um caractere: 744.000 casos, todos recusados.
- Troca de dois vizinhos: 20.314 casos, 39 passaram, todos da troca entre `0` e `Z`. É o limite conhecido do Luhn mod N; o teste `limite_conhecido_troca_de_0_com_z_passa` registra esse caso.

## O que não resolve

- Com 35 bits, dá para fabricar uma chave que caia no código de outra pessoa. Por isso o servidor não pode aceitar dois cadastros com o mesmo código, e o código nunca substitui a conferência da chave.
- Duas pessoas honestas podem cair no mesmo código. Pela conta do aniversário: 0,15% de chance de haver alguma colisão com 10 mil usuários e 50% perto de 218 mil. Na primeira abertura, quando o servidor recusar o código por já estar em uso, o aparelho gera outra identidade.
- Desde 07/11/2026 a identidade fica gravada cifrada no aparelho (PIN + Keystore) e o código não muda entre aberturas. Ver `cofre-da-identidade.md`.
