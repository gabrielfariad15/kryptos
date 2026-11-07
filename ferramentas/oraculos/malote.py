# ferramentas/oraculos/malote.py
# Oraculo da busca no malote do relay: monta e assina o vetor de protocolo/src
# (vetor_busca.hex) sem usar o Rust.
#
# Uso (so quando o formato mudar):
#   python ferramentas/oraculos/malote.py <pasta onde gravar o .hex>
#
# Precisa do pacote `cryptography` (ja listado em documentacao/DEPENDENCIAS.md, so para
# oraculos). Formato: documentacao/arquitetura/malote.md.
#
# Calibracao (regra 5.3): a assinatura Ed25519 daqui tem de bater com o teste 1 da
# RFC 8032 (mensagem vazia) antes de assinar o vetor.

import struct
import sys
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

SECRETA = bytes.fromhex(
    "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
PUBLICA_RFC = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
ASSINATURA_RFC = (
    "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
    "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b")

VERSAO = 1
DOMINIO = b"KRYPTOS/busca/v1"


def corpo(destinatario, numero, ids):
    c = bytes([VERSAO]) + destinatario + struct.pack("<Q", numero) + struct.pack("<H", len(ids))
    for i in ids:
        assert len(i) == 16
        c += i
    return c


def main():
    if len(sys.argv) != 2:
        print("uso: python malote.py <pasta>")
        return 2
    secreta = Ed25519PrivateKey.from_private_bytes(SECRETA)
    dono = secreta.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    if dono.hex() != PUBLICA_RFC or secreta.sign(b"").hex() != ASSINATURA_RFC:
        print("FALHA calibracao: Ed25519 nao bate com o teste 1 da RFC 8032")
        return 1
    print("OK calibracao (RFC 8032, teste 1)")
    c = corpo(dono, 7, [b"\x11" * 16, b"\x22" * 16])
    busca = c + secreta.sign(DOMINIO + c)
    (Path(sys.argv[1]) / "vetor_busca.hex").write_bytes((busca.hex() + "\n").encode("ascii"))
    print(f"OK vetor_busca.hex: {len(busca)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
