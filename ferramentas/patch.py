#!/usr/bin/env python3
"""Aplica trocas de texto num arquivo do repositorio, simulando antes.

Uso:
  python ferramentas/patch.py troca.json             (so simula, nao grava)
  python ferramentas/patch.py troca.json --aplicar   (grava e confere)

Formato do troca.json:
{
  "arquivo": "nucleo/src/lib.rs",
  "sha256_antes": "opcional: 12 ou mais caracteres do sha do arquivo atual",
  "trocas": [
    {"antes": "texto exato que existe hoje", "depois": "texto novo"}
  ]
}

Regras:
- Cada "antes" precisa aparecer exatamente 1 vez (contado depois das trocas anteriores).
  Zero ou mais de uma vez: aborta sem gravar nada.
- Se "sha256_antes" vier e nao bater, aborta (o arquivo mudou desde que foi lido).
- Mantem BOM e quebra de linha (LF ou CRLF) do arquivo original.
- Confere a sintaxe do resultado quando sabe (.py, .json, .toml). Para outros
  tipos avisa que nao conferiu (ex.: .rs pede cargo check).
- Com --aplicar: grava num temporario, troca atomicamente e le de volta.

Saida: 0 = ok, 1 = abortou, 2 = uso errado.
"""

import hashlib
import json
import os
import sys
import tempfile

BOM = b"\xef\xbb\xbf"


def sha(dados: bytes) -> str:
    return hashlib.sha256(dados).hexdigest()


def falha(msg: str) -> None:
    print("ABORTADO: " + msg)
    print("RESULTADO: FALHOU (nada foi gravado)")
    sys.exit(1)


def conferir_sintaxe(caminho: str, texto: str) -> str:
    ext = os.path.splitext(caminho)[1].lower()
    try:
        if ext == ".py":
            compile(texto, caminho, "exec")
            return "sintaxe Python ok"
        if ext == ".json":
            json.loads(texto)
            return "JSON ok"
        if ext == ".toml":
            try:
                import tomllib
            except ImportError:
                return "TOML nao conferido (Python sem tomllib)"
            tomllib.loads(texto)
            return "TOML ok"
    except Exception as erro:  # o erro e impresso, nao engolido
        falha("sintaxe invalida depois das trocas: %s: %s" % (type(erro).__name__, erro))
    return "sintaxe NAO conferida para %s (rode a checagem da linguagem)" % (ext or "arquivo sem extensao")


def main() -> None:
    args = [a for a in sys.argv[1:] if a != "--aplicar"]
    aplicar = "--aplicar" in sys.argv[1:]
    if len(args) != 1:
        print(__doc__)
        sys.exit(2)

    try:
        with open(args[0], "r", encoding="utf-8") as f:
            spec = json.load(f)
    except Exception as erro:
        falha("nao consegui ler %s: %s" % (args[0], erro))

    if not isinstance(spec, dict) or "arquivo" not in spec or not spec.get("trocas"):
        falha("o json precisa de 'arquivo' e de uma lista 'trocas' nao vazia")

    raiz = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    alvo = spec["arquivo"]
    if not os.path.isabs(alvo):
        alvo = os.path.join(raiz, alvo)
    if not os.path.isfile(alvo):
        falha("arquivo nao existe: %s" % alvo)

    with open(alvo, "rb") as f:
        bruto = f.read()
    sha_antes = sha(bruto)

    esperado = spec.get("sha256_antes")
    if esperado:
        esperado = esperado.lower()
        if len(esperado) < 12 or not sha_antes.startswith(esperado):
            falha("sha do arquivo e %s, o patch esperava %s" % (sha_antes[:12], esperado[:12]))

    tem_bom = bruto.startswith(BOM)
    corpo = bruto[len(BOM):] if tem_bom else bruto
    try:
        texto = corpo.decode("utf-8")
    except UnicodeDecodeError as erro:
        falha("arquivo nao e UTF-8: %s" % erro)

    crlf = "\r\n" in texto
    linhas_antes = texto.count("\n") + 1

    for i, troca in enumerate(spec["trocas"], start=1):
        if not isinstance(troca, dict) or "antes" not in troca or "depois" not in troca:
            falha("troca %d precisa de 'antes' e 'depois'" % i)
        antes = troca["antes"].replace("\r\n", "\n")
        depois = troca["depois"].replace("\r\n", "\n")
        if crlf:
            antes = antes.replace("\n", "\r\n")
            depois = depois.replace("\n", "\r\n")
        if not antes:
            falha("troca %d tem 'antes' vazio" % i)
        n = texto.count(antes)
        if n != 1:
            inicio = antes.strip().splitlines()[0][:60] if antes.strip() else repr(antes[:60])
            falha("troca %d: ancora aparece %d vezes (precisa ser 1). Inicio: %s" % (i, n, inicio))
        texto = texto.replace(antes, depois, 1)
        print("  troca %d: ancora unica, aplicada na simulacao" % i)

    nota = conferir_sintaxe(alvo, texto)
    novo = (BOM if tem_bom else b"") + texto.encode("utf-8")
    sha_depois = sha(novo)

    print("  arquivo: %s" % os.path.relpath(alvo, raiz))
    print("  linhas: %d -> %d   quebra: %s   BOM: %s" % (
        linhas_antes, texto.count("\n") + 1, "CRLF" if crlf else "LF", "sim" if tem_bom else "nao"))
    print("  sha: %s -> %s" % (sha_antes[:12], sha_depois[:12]))
    print("  " + nota)

    if not aplicar:
        print("RESULTADO: SIMULACAO OK (nada gravado; rode de novo com --aplicar)")
        return

    pasta = os.path.dirname(alvo)
    fd, temporario = tempfile.mkstemp(dir=pasta, prefix=".patch-", suffix=".tmp")
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(novo)
        os.replace(temporario, alvo)
    finally:
        if os.path.exists(temporario):
            os.remove(temporario)

    with open(alvo, "rb") as f:
        relido = sha(f.read())
    if relido != sha_depois:
        print("ERRO: li de volta e o sha e %s, esperado %s" % (relido[:12], sha_depois[:12]))
        print("RESULTADO: FALHOU")
        sys.exit(1)
    print("RESULTADO: GRAVADO E CONFERIDO (sha %s)" % relido[:12])


if __name__ == "__main__":
    main()
