# ferramentas/oraculos/cofre.py
# Oraculo do cofre da identidade: calcula os vetores conhecidos de nucleo/src/cofre.rs
# sem usar o codigo Rust. So bibliotecas Python (argon2-cffi e cryptography).
#
# Uso (so quando o formato do cofre mudar):
#   pip install argon2-cffi cryptography
#   python ferramentas/oraculos/cofre.py
#
# A conta fixa usa a chave Ed25519 do teste 1 da RFC 8032 e a X25519 da Alice da
# RFC 7748, as mesmas do vetor do codigo de contato: o codigo esperado e 2J2F-P63Y.

import base64
import hashlib
import hmac
import json
import struct
import sys

from argon2.low_level import Type, hash_secret_raw
from cryptography.hazmat.primitives import padding
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes


def hkdf_sha256(salt: bytes, ikm: bytes, info: bytes, n: int) -> bytes:
    """RFC 5869, escrito aqui para nao depender da mesma biblioteca do Rust."""
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    saida, bloco, i = b"", b"", 1
    while len(saida) < n:
        bloco = hmac.new(prk, bloco + info + bytes([i]), hashlib.sha256).digest()
        saida += bloco
        i += 1
    return saida[:n]


def argon2id(pin: bytes, sal: bytes, m: int, t: int, p: int) -> bytes:
    return hash_secret_raw(pin, sal, time_cost=t, memory_cost=m, parallelism=p,
                           hash_len=32, type=Type.ID, version=19)


def cabecalho(sal: bytes, m: int, t: int, p: int, nonce: bytes) -> bytes:
    return b"KRYCOFRE" + b"\x01" + sal + struct.pack("<III", m, t, p) + nonce


def cifra_de_conta_vodozemac(chave: bytes, texto: bytes) -> str:
    """Cifra de conta do vodozemac (formato nativo): HKDF com sal [0] e info "Pickle",
    80 bytes = chave AES | chave HMAC | IV; AES-256-CBC com PKCS7; HMAC-SHA256
    truncado em 8 bytes; base64 sem preenchimento."""
    k = hkdf_sha256(b"\x00", chave, b"Pickle", 80)
    aes, mac, iv = k[0:32], k[32:64], k[64:80]
    preench = padding.PKCS7(128).padder()
    claro = preench.update(texto) + preench.finalize()
    cif = Cipher(algorithms.AES(aes), modes.CBC(iv)).encryptor()
    ct = cif.update(claro) + cif.finalize()
    tag = hmac.new(mac, ct, hashlib.sha256).digest()[:8]
    return base64.b64encode(ct + tag).decode().rstrip("=")


ED25519_RFC8032_T1 = bytes.fromhex(
    "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
X25519_ALICE_RFC7748 = bytes.fromhex(
    "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")

# Mesma ordem de campos em que o vodozemac 0.11.2 serializa a conta (serde_json).
CONTA = {
    "signing_key": {"Normal": list(ED25519_RFC8032_T1)},
    "diffie_hellman_key": list(X25519_ALICE_RFC7748),
    "one_time_keys": {"next_key_id": 0, "public_keys": {}, "private_keys": {}},
    "fallback_keys": {"key_id": 0, "fallback_key": None, "previous_fallback_key": None},
}


def main() -> int:
    sal0 = bytes(range(16))
    salA = bytes(0xA0 + i for i in range(16))
    nonce = bytes(0x40 + i for i in range(16))
    chave_do_cofre = bytes(0x20 + i for i in range(32))
    m, t, p = 19456, 2, 1

    cab = cabecalho(sal0, m, t, p, nonce)
    chave_conta = hkdf_sha256(nonce, chave_do_cofre, b"KRYPTOS/cofre/v1/conta" + cab, 32)
    texto = json.dumps(CONTA, separators=(",", ":")).encode()
    arquivo = cab + cifra_de_conta_vodozemac(chave_conta, texto).encode()

    print("ARGON2_BARATO", argon2id(b"123456", sal0, 64, 1, 1).hex())
    print("ARGON2_PADRAO", argon2id(b"908172", salA, m, t, p).hex())
    print("CABECALHO", cab.hex())
    print("CHAVE_DA_CONTA", chave_conta.hex())
    print("ARQUIVO", arquivo.hex())
    print("CONTA_FIXA", texto.decode())
    return 0


if __name__ == "__main__":
    sys.exit(main())
