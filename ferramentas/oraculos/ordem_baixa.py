# ferramentas/oraculos/ordem_baixa.py
# Oraculo das chaves Curve25519 de ordem baixa: as codificacoes de 32 bytes que zeram
# o X25519 para qualquer chave secreta. Gera a lista de nucleo/src/sessao.rs.
#
# Uso (so quando a lista precisar ser refeita):
#   python ferramentas/oraculos/ordem_baixa.py
#
# So a biblioteca padrao. O X25519 e escrito aqui de novo pela RFC 7748 (secao 5),
# sem olhar o Rust. Os cinco valores de u de ordem baixa sao os da lista publica do
# libsodium; o oraculo nao confia nela: confere cada codificacao no X25519.
#
# Calibracao (regra 5.3):
#   caso certo:  o X25519 daqui bate com o vetor 1 da secao 5.2 da RFC 7748;
#   caso errado: chaves comuns (a da Alice da RFC 7748 e 64 sorteadas) nao zeram.

import secrets
import sys

P = 2**255 - 19
A24 = 121665

# u de ordem 1, 2, 4 ou 8, na curva ou na torcao (lista do libsodium).
U_DE_ORDEM_BAIXA = [
    0,
    1,
    325606250916557431795983626356110631294008115727848805560023387167927233504,
    39382357235489614581723060781553021112529911719440698176882885853963445705823,
    P - 1,
]


def decodificar_u(b):
    """RFC 7748: ignora o bit 255."""
    return int.from_bytes(b, "little") & ((1 << 255) - 1)


def decodificar_escalar(b):
    k = bytearray(b)
    k[0] &= 248
    k[31] &= 127
    k[31] |= 64
    return int.from_bytes(k, "little")


def x25519(k_bytes, u_bytes):
    """Escada de Montgomery da RFC 7748, secao 5."""
    k = decodificar_escalar(k_bytes)
    x1 = decodificar_u(u_bytes) % P
    x2, z2, x3, z3 = 1, 0, x1, 1
    troca = 0
    for t in reversed(range(255)):
        bit = (k >> t) & 1
        troca ^= bit
        if troca:
            x2, x3, z2, z3 = x3, x2, z3, z2
        troca = bit
        a = (x2 + z2) % P
        aa = a * a % P
        b = (x2 - z2) % P
        bb = b * b % P
        e = (aa - bb) % P
        c = (x3 + z3) % P
        d = (x3 - z3) % P
        da = d * a % P
        cb = c * b % P
        x3 = (da + cb) ** 2 % P
        z3 = x1 * (da - cb) ** 2 % P
        x2 = aa * bb % P
        z2 = e * (aa + A24 * e) % P
    if troca:
        x2, x3, z2, z3 = x3, x2, z3, z2
    return (x2 * pow(z2, P - 2, P) % P).to_bytes(32, "little")


def codificacoes():
    """Cada u de ordem baixa, em todas as grafias de 32 bytes que o X25519 aceita:
    u e u + p (quando cabe em 255 bits), com o bit 255 em zero e em um."""
    saida = []
    for u in U_DE_ORDEM_BAIXA:
        for valor in (u, u + P):
            if valor >= 2**255:
                continue
            for alto in (0, 1 << 255):
                saida.append((valor | alto).to_bytes(32, "little"))
    return saida


def calibrar():
    k = bytes.fromhex("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4")
    u = bytes.fromhex("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c")
    if x25519(k, u).hex() != (
            "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552"):
        return "X25519 daqui nao bate com a RFC 7748, secao 5.2"
    alice = bytes.fromhex("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")
    comuns = [alice] + [secrets.token_bytes(32) for _ in range(64)]
    for chave in comuns:
        if x25519(secrets.token_bytes(32), chave) == bytes(32):
            return "chave comum zerou o X25519"
    return None


def main():
    erro = calibrar()
    if erro:
        print(f"FALHA calibracao: {erro}")
        return 1
    print("OK calibracao (RFC 7748 e 65 chaves comuns)")
    lista = codificacoes()
    if len(set(lista)) != len(lista):
        print("FALHA lista com repeticao")
        return 1
    for chave in lista:
        for _ in range(16):
            if x25519(secrets.token_bytes(32), chave) != bytes(32):
                print(f"FALHA {chave.hex()} nao zera")
                return 1
    print(f"OK {len(lista)} codificacoes zeram o X25519 com 16 chaves secretas sorteadas")
    for chave in lista:
        print(chave.hex())
    return 0


if __name__ == "__main__":
    sys.exit(main())
