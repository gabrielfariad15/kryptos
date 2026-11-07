import 'package:flutter/material.dart';

import 'cofre.dart';
import 'tema.dart';
import 'telas/cofre_aberto.dart';
import 'telas/desbloqueio.dart';
import 'telas/primeira_abertura.dart';

/// App sem dependência direta da ponte: recebe a versão do núcleo pronta, a função
/// que gera o código e o cofre, para os testes de tela rodarem no computador sem a
/// biblioteca em Rust.
///
/// Sem cofre gravado: primeira abertura. Com cofre: desbloqueio. Depois de um dos
/// dois: o cofre aberto, com o código.
class KryptosApp extends StatelessWidget {
  const KryptosApp({
    super.key,
    required this.versaoNucleo,
    required this.gerarCodigo,
    required this.cofre,
  });

  final String versaoNucleo;
  final String Function() gerarCodigo;
  final Cofre cofre;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Kryptos',
      debugShowCheckedModeBanner: false,
      theme: temaKryptos(),
      home: _Entrada(
        versaoNucleo: versaoNucleo,
        gerarCodigo: gerarCodigo,
        cofre: cofre,
      ),
    );
  }
}

class _Entrada extends StatefulWidget {
  const _Entrada({
    required this.versaoNucleo,
    required this.gerarCodigo,
    required this.cofre,
  });

  final String versaoNucleo;
  final String Function() gerarCodigo;
  final Cofre cofre;

  @override
  State<_Entrada> createState() => _EntradaState();
}

class _EntradaState extends State<_Entrada> {
  late final bool _temCofre = widget.cofre.existe();
  String? _codigo;

  void _aberto(String codigo) => setState(() => _codigo = codigo);

  @override
  Widget build(BuildContext context) {
    final codigo = _codigo;
    if (codigo != null) {
      return CofreAberto(versaoNucleo: widget.versaoNucleo, codigo: codigo);
    }
    if (_temCofre) {
      return Desbloqueio(
        versaoNucleo: widget.versaoNucleo,
        cofre: widget.cofre,
        aoAbrir: _aberto,
      );
    }
    return PrimeiraAbertura(
      versaoNucleo: widget.versaoNucleo,
      gerarCodigo: widget.gerarCodigo,
      cofre: widget.cofre,
      aoConcluir: _aberto,
    );
  }
}
