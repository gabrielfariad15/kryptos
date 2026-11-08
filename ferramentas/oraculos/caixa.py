# ferramentas/oraculos/caixa.py
# Oraculo da caixa selada: monta a gaveta e o indice dos vetores conhecidos de
# nucleo/src/caixa.rs (teste vetores_conhecidos) sem usar o Rust e imprime o SHA-256
# de cada um.
#
# Uso (so quando o formato mudar):
#   python ferramentas/oraculos/caixa.py
#
# Usa o XChaCha20-Poly1305 do oraculo da agenda (agenda.py, mesma pasta), que se
# calibra contra os vetores do draft-irtf-cfrg-xchacha-03 antes de qualquer conta.
# Formato: documentacao/arquitetura/caixa-selada.md.

import hashlib
import importlib.util
import struct
import sys
from pathlib import Path

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

# Sem __pycache__ na pasta do repositorio.
sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location(
    "agenda_oraculo", Path(__file__).with_name("agenda.py"))
agenda = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(agenda)

CAPACIDADE = 100
MAXIMO_TEXTO = 65536
DOMINIO_GAVETA = b"KRYPTOS/gaveta/v1"
INFO_INDICE = b"KRYPTOS/caixa/v1"

CHAVE_DO_COFRE = bytes([3] * 32)
HMAC = bytes([5] * 32)
NONCE = bytes([8] * 24)
ED = bytes.fromhex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
CV = bytes([0x85] * 32)


def hkdf(ikm, info):
    return HKDF(algorithm=hashes.SHA256(), length=32, salt=None, info=info).derive(ikm)


def gaveta(numero, texto):
    claro = ED + CV + struct.pack("<I", len(texto)) + texto
    claro += bytes(64 + 4 + MAXIMO_TEXTO - len(claro))
    aad = DOMINIO_GAVETA + bytes([numero])
    return NONCE + agenda.xchacha20poly1305(hkdf(HMAC, DOMINIO_GAVETA), NONCE, claro, aad)


def indice(versao):
    # (estado, numero, resumo, remetente, chegada); versao 1 nao tem remetente nem
    # chegada, versao 2 nao tem chegada (minuto desde 1970, u32 LE).
    lugares = [(0, 0, bytes(32), bytes(32), 0)] * CAPACIDADE
    lugares[0] = (2, 1, bytes([1] * 32), bytes([0x40] * 32), 29_000_000)  # nao lida
    lugares[5] = (3, 2, bytes([6] * 32), bytes([0x45] * 32), 29_000_005)  # lida
    lugares[99] = (1, 3, bytes([100] * 32), bytes(32), 0)   # reservada, sem remetente
    lugares[7] = (4, 0, bytes(32), bytes(32), 0)            # queimada
    claro = struct.pack("<Q", 4)
    for estado, numero, resumo, remetente, chegada in lugares:
        claro += bytes([estado]) + struct.pack("<Q", numero) + resumo
        if versao >= 2:
            claro += remetente
        if versao >= 3:
            claro += struct.pack("<I", chegada)
    cabecalho = b"KRYCAIXA" + bytes([versao]) + NONCE
    return cabecalho + agenda.xchacha20poly1305(
        hkdf(CHAVE_DO_COFRE, INFO_INDICE), NONCE, claro, cabecalho)


def main():
    agenda.calibrar()
    g = gaveta(7, b"Kryptos")
    i3 = indice(3)
    i2 = indice(2)
    i1 = indice(1)
    print("calibracao: ok")
    print("gaveta:", len(g), hashlib.sha256(g).hexdigest())
    print("indice v3:", len(i3), hashlib.sha256(i3).hexdigest())
    print("indice v2:", len(i2), hashlib.sha256(i2).hexdigest())
    print("indice v1:", len(i1), hashlib.sha256(i1).hexdigest())


if __name__ == "__main__":
    main()
