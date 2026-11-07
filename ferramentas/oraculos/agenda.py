# ferramentas/oraculos/agenda.py
# Oraculo da agenda gravada: monta o arquivo do vetor conhecido de
# nucleo/src/agenda_gravada.rs (teste vetor_conhecido) sem usar o Rust e imprime o
# SHA-256 dele.
#
# Uso (so quando o formato mudar):
#   python ferramentas/oraculos/agenda.py
#
# Precisa do pacote `cryptography` (ja listado em documentacao/DEPENDENCIAS.md, so para
# oraculos). Formato: documentacao/arquitetura/agenda-gravada.md.
#
# O `cryptography` nao tem XChaCha20-Poly1305. Aqui ele e montado como diz o rascunho
# draft-irtf-cfrg-xchacha-03: HChaCha20 (escrito a mao) tira uma subchave dos 16
# primeiros bytes do nonce, e o ChaCha20-Poly1305 do `cryptography` cifra com o nonce
# 0000 0000 || ultimos 8 bytes.
#
# Calibracao (regra 5.3), antes de montar o vetor:
# - HChaCha20 contra o vetor da secao 2.2.1 do rascunho;
# - XChaCha20-Poly1305 contra a etiqueta do vetor da secao A.3.1 do rascunho.

import hashlib
import struct
import sys

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

MAXIMO_CONTATOS = 5000
BYTES_POR_CONTATO = 1 + 32 + 32 + 1 + 32
MAGICO = b"KRYAGEND"
VERSAO = 1
INFO = b"KRYPTOS/agenda/v1"

CHAVE_DO_COFRE = bytes([7] * 32)
NONCE = bytes([9] * 24)
CONTATOS = [
    ("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
     "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"),
    ("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
     "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f"),
]


def _rot(v, n):
    return ((v << n) & 0xFFFFFFFF) | (v >> (32 - n))


def _quarto(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & 0xFFFFFFFF; s[d] = _rot(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & 0xFFFFFFFF; s[b] = _rot(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b]) & 0xFFFFFFFF; s[d] = _rot(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & 0xFFFFFFFF; s[b] = _rot(s[b] ^ s[c], 7)


def hchacha20(chave, nonce16):
    assert len(chave) == 32 and len(nonce16) == 16
    s = [0x61707865, 0x3320646E, 0x79622D32, 0x6B206574]
    s += list(struct.unpack("<8I", chave)) + list(struct.unpack("<4I", nonce16))
    for _ in range(10):
        _quarto(s, 0, 4, 8, 12); _quarto(s, 1, 5, 9, 13)
        _quarto(s, 2, 6, 10, 14); _quarto(s, 3, 7, 11, 15)
        _quarto(s, 0, 5, 10, 15); _quarto(s, 1, 6, 11, 12)
        _quarto(s, 2, 7, 8, 13); _quarto(s, 3, 4, 9, 14)
    return struct.pack("<8I", *(s[0:4] + s[12:16]))


def xchacha20poly1305(chave, nonce24, claro, aad):
    sub = hchacha20(chave, nonce24[:16])
    return ChaCha20Poly1305(sub).encrypt(b"\0\0\0\0" + nonce24[16:], claro, aad)


def calibrar():
    h = hchacha20(bytes(range(32)), bytes.fromhex("000000090000004a0000000031415927"))
    esperado = "82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc"
    if h.hex() != esperado:
        sys.exit("CALIBRACAO FALHOU: HChaCha20 " + h.hex())
    texto = (b"Ladies and Gentlemen of the class of '99: If I could offer you only one "
             b"tip for the future, sunscreen would be it.")
    c = xchacha20poly1305(bytes(range(0x80, 0xA0)),
                          bytes(range(0x40, 0x58)), texto,
                          bytes.fromhex("50515253c0c1c2c3c4c5c6c7"))
    if c[-16:].hex() != "c0875924c1c7987947deafd8780acf49":
        sys.exit("CALIBRACAO FALHOU: etiqueta XChaCha20-Poly1305 " + c[-16:].hex())


def main():
    calibrar()
    chave = HKDF(algorithm=hashes.SHA256(), length=32, salt=None, info=INFO).derive(
        CHAVE_DO_COFRE)
    claro = bytearray(2 + MAXIMO_CONTATOS * BYTES_POR_CONTATO)
    claro[0:2] = struct.pack("<H", len(CONTATOS))
    for i, (ed, cv) in enumerate(CONTATOS):
        p = 2 + i * BYTES_POR_CONTATO
        claro[p] = 1
        claro[p + 1:p + 33] = bytes.fromhex(ed)
        claro[p + 33:p + 65] = bytes.fromhex(cv)
    cabecalho = MAGICO + bytes([VERSAO]) + NONCE
    arquivo = cabecalho + xchacha20poly1305(chave, NONCE, bytes(claro), cabecalho)
    print("calibracao: ok")
    print("tamanho:", len(arquivo))
    print("sha256:", hashlib.sha256(arquivo).hexdigest())


if __name__ == "__main__":
    main()
