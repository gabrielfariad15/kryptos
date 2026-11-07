# ferramentas/oraculos/olm.py
# Oraculo da sessao Olm (conversa entre duas pessoas): monta a primeira mensagem de A
# para B seguindo a especificacao publica do Olm v1, sem usar o vodozemac nem o Rust.
# Gera o vetor conhecido de nucleo/src/sessao.rs (regra 8.4).
#
# Uso (so quando o vetor precisar ser refeito):
#   python ferramentas/oraculos/olm.py
#
# Precisa do pacote `cryptography` (ja listado em documentacao/DEPENDENCIAS.md, so
# para oraculos). Especificacao seguida:
#   https://gitlab.matrix.org/matrix-org/olm/-/blob/master/docs/olm.md
#
# Calibracao antes de gerar (regra 5.3): X25519 contra a secao 6.1 da RFC 7748 e
# HKDF-SHA256 contra o caso 1 da RFC 5869. Se uma das duas falhar, nada e impresso
# alem da FALHA.

import hashlib
import hmac
import sys

from cryptography.hazmat.primitives import hashes, padding
from cryptography.hazmat.primitives.asymmetric.x25519 import (
    X25519PrivateKey,
    X25519PublicKey,
)
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

# --- chaves fixas ---------------------------------------------------------------
# B: a mesma conta fixa do cofre (CONTA_FIXA_JSON em nucleo/src/cofre.rs):
#    Curve25519 secreta da Alice da RFC 7748. A chave avulsa de B e fixa daqui.
B_IDENTIDADE = bytes.fromhex(
    "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")
B_AVULSA = bytes(range(0x50, 0x70))
# A: Curve25519 secreta do Bob da RFC 7748; chave base e de catraca fixas daqui.
A_IDENTIDADE = bytes.fromhex(
    "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb")
A_BASE = bytes(range(0x70, 0x90))
A_CATRACA = bytes(range(0x90, 0xB0))

TEXTO = b"KRYPTOS vetor 1: A para B"


def publica(secreta):
    return X25519PrivateKey.from_private_bytes(secreta).public_key().public_bytes(
        Encoding.Raw, PublicFormat.Raw)


def dh(secreta, publica_outro):
    return X25519PrivateKey.from_private_bytes(secreta).exchange(
        X25519PublicKey.from_public_bytes(publica_outro))


def hkdf(sal, chave, info, tamanho):
    return HKDF(algorithm=hashes.SHA256(), length=tamanho, salt=sal, info=info).derive(chave)


def varint(n):
    saida = bytearray()
    while True:
        byte = n & 0x7F
        n >>= 7
        if n:
            saida.append(byte | 0x80)
        else:
            saida.append(byte)
            return bytes(saida)


def campo_bytes(numero, valor):
    """Campo protobuf do tipo 2 (comprimento + bytes)."""
    return bytes([(numero << 3) | 2]) + varint(len(valor)) + valor


def campo_inteiro(numero, valor):
    """Campo protobuf do tipo 0 (varint)."""
    return bytes([(numero << 3) | 0]) + varint(valor)


def calibrar():
    # RFC 7748, secao 6.1: segredo compartilhado da Alice com o Bob.
    esperado = bytes.fromhex(
        "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742")
    if dh(B_IDENTIDADE, publica(A_IDENTIDADE)) != esperado:
        return "X25519 nao bate com a RFC 7748"
    if publica(B_IDENTIDADE).hex() != (
            "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"):
        return "chave publica da Alice nao bate com a RFC 7748"
    # RFC 5869, caso 1.
    okm = hkdf(bytes(range(0x0D)), bytes([0x0B] * 22), bytes(range(0xF0, 0xFA)), 42)
    if okm.hex() != ("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf"
                     "34007208d5b887185865"):
        return "HKDF nao bate com a RFC 5869"
    return None


def primeira_mensagem():
    b_identidade_pub = publica(B_IDENTIDADE)
    b_avulsa_pub = publica(B_AVULSA)
    a_identidade_pub = publica(A_IDENTIDADE)
    a_base_pub = publica(A_BASE)
    a_catraca_pub = publica(A_CATRACA)

    # Especificacao, "Initial setup": S = ECDH(I_A, E_B) || ECDH(E_A, I_B) || ECDH(E_A, E_B)
    s = (dh(A_IDENTIDADE, b_avulsa_pub) + dh(A_BASE, b_identidade_pub)
         + dh(A_BASE, b_avulsa_pub))
    # R_0, C_{0,0} = HKDF(0, S, "OLM_ROOT", 64); o sal e o byte 0.
    raiz_e_cadeia = hkdf(b"\x00", s, b"OLM_ROOT", 64)
    cadeia = raiz_e_cadeia[32:]

    # "Advancing the chain key" e "Creating a message key": M_{i,j} = HMAC(C_{i,j}, "\x01")
    chave_mensagem = hmac.new(cadeia, b"\x01", hashlib.sha256).digest()
    # "Olm Message keys": AES_KEY || HMAC_KEY || AES_IV = HKDF(0, M, "OLM_KEYS", 80)
    material = hkdf(b"\x00", chave_mensagem, b"OLM_KEYS", 80)
    chave_aes, chave_mac, iv = material[:32], material[32:64], material[64:80]

    enchimento = padding.PKCS7(128).padder()
    claro = enchimento.update(TEXTO) + enchimento.finalize()
    cifra = Cipher(algorithms.AES(chave_aes), modes.CBC(iv)).encryptor()
    cifrado = cifra.update(claro) + cifra.finalize()

    # "Normal message": versao 3, catraca (1), indice da cadeia (2), texto cifrado (4),
    # e o MAC de 8 bytes (versao 1 da sessao: HMAC-SHA256 truncado).
    corpo = (b"\x03" + campo_bytes(1, a_catraca_pub) + campo_inteiro(2, 0)
             + campo_bytes(4, cifrado))
    mac = hmac.new(chave_mac, corpo, hashlib.sha256).digest()[:8]
    interna = corpo + mac

    # "Pre-key message": versao 3, chave avulsa (1), chave base (2), identidade (3),
    # mensagem (4). Sem MAC proprio: a mensagem interna e que confere.
    pre_chave = (b"\x03" + campo_bytes(1, b_avulsa_pub) + campo_bytes(2, a_base_pub)
                 + campo_bytes(3, a_identidade_pub) + campo_bytes(4, interna))
    return {
        "B_AVULSA_SECRETA": B_AVULSA,
        "B_AVULSA_PUBLICA": b_avulsa_pub,
        "A_IDENTIDADE_PUBLICA": a_identidade_pub,
        "PRIMEIRA_MENSAGEM": pre_chave,
    }


def main():
    erro = calibrar()
    if erro:
        print(f"FALHA calibracao: {erro}")
        return 1
    print("OK calibracao (RFC 7748 e RFC 5869)")
    for nome, valor in primeira_mensagem().items():
        print(f"{nome} = {valor.hex()}")
    print(f"TEXTO = {TEXTO.decode('ascii')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
