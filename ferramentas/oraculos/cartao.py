# ferramentas/oraculos/cartao.py
# Oraculo do cartao de contato (o texto dentro do QR): calcula o vetor conhecido de
# nucleo/src/cartao.rs sem usar o codigo Rust. So a biblioteca padrao do Python.
#
# Uso (so quando o formato do cartao mudar):
#   python ferramentas/oraculos/cartao.py
#
# Chaves fixas: Ed25519 publica do teste 1 da RFC 8032 e X25519 publica da Alice da
# RFC 7748, as mesmas do vetor do codigo de contato. Calibracao: o codigo tem de sair
# 2J2F-P63Y, o valor ja conferido em documentacao/arquitetura/codigo-de-contato.md.

import hashlib
import sys

ALFABETO = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

ED25519 = bytes.fromhex(
    "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
CURVE25519 = bytes.fromhex(
    "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")

PREFIXO = "KRYPTOS1"
CODIGO_CONFERIDO = "2J2F-P63Y"


def luhn_mod_32(valores):
    """Digito verificador Luhn mod N (N = 32), escrito aqui de novo, sem olhar o Rust."""
    fator, soma = 2, 0
    for v in reversed(valores):
        parcela = fator * v
        soma += parcela // 32 + parcela % 32
        fator = 1 if fator == 2 else 2
    return (32 - soma % 32) % 32


def codigo(ed, cv):
    h = hashlib.sha256(b"KRYPTOS/codigo-de-contato/v1" + ed + cv).digest()
    bits = int.from_bytes(h[:5], "big") >> 5
    valores = [(bits >> (5 * (6 - i))) & 0x1F for i in range(7)]
    valores.append(luhn_mod_32(valores))
    texto = "".join(ALFABETO[v] for v in valores)
    return texto[:4] + "-" + texto[4:]


def base32_chaves(dados):
    """64 bytes = 512 bits; completa com 3 bits zero ate 515 = 103 caracteres."""
    assert len(dados) == 64
    n = int.from_bytes(dados, "big") << 3
    return "".join(ALFABETO[(n >> (5 * (102 - i))) & 0x1F] for i in range(103))


def cartao(ed, cv):
    return PREFIXO + "-" + base32_chaves(ed + cv) + "-" + codigo(ed, cv).replace("-", "")


def main():
    c = codigo(ED25519, CURVE25519)
    if c != CODIGO_CONFERIDO:
        print(f"FALHA calibracao: codigo {c}, esperado {CODIGO_CONFERIDO}")
        return 1
    texto = cartao(ED25519, CURVE25519)
    print(f"OK    calibracao: codigo {c}")
    print(f"cartao ({len(texto)} caracteres):")
    print(texto)
    return 0


if __name__ == "__main__":
    sys.exit(main())
