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

## O que não resolve

- **Chave Curve25519 de ordem baixa não é recusada na leitura.** Quem recusa é o vodozemac, ao abrir a sessão (`SessionCreationError::NonContributoryKey`, conferido no código da 0.11.2). Fica para o assunto da conversa provar com teste.
- **Quem mostra o QR pode mostrar a chave de outra pessoa.** Ela não ganha nada com isso, porque não tem a chave secreta, mas o contato fica com o nome errado. A leitura pessoal prova "esta é a chave que este aparelho mostra", não "este aparelho é desta pessoa".
- **Aparelho comprometido** mostra a chave que o atacante quiser.
- **Print da tela:** o QR é a chave pública, não é segredo, mas quem fotografar a tela ganha o contato. O `FLAG_SECURE` ainda não está no app (a captura do G15 de 07/11/2026 saiu normal).
